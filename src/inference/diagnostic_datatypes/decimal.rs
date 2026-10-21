use super::context::*;
use super::{LexicalFailure, LexicalResult, unicode};

struct Finite {
    digits: String,
    exponent: i128,
}

enum Parsed {
    Finite(Finite),
    Infinity,
    Nan { signaling: bool },
}

fn parse(text: &str) -> Option<Parsed> {
    let mut normalized = String::new(); // bounded: this one scalar lexical value.
    for ch in text
        .trim_matches(unicode::whitespace)
        .chars()
        .filter(|&c| c != '_')
    {
        if let Some(digit) = unicode::decimal(ch) {
            normalized.push(char::from(b'0' + digit));
        } else {
            normalized.push(ch.to_ascii_lowercase());
        }
    }
    let value = normalized.trim_start_matches(['+', '-']);
    if normalized.len() - value.len() > 1 {
        return None;
    }
    if matches!(value, "inf" | "infinity") {
        return Some(Parsed::Infinity);
    }
    if let Some(payload) = value
        .strip_prefix("snan")
        .or_else(|| value.strip_prefix("nan"))
    {
        return payload
            .bytes()
            .all(|c| c.is_ascii_digit())
            .then_some(Parsed::Nan {
                signaling: value.starts_with("snan"),
            });
    }
    let (mantissa, exponent) = if let Some((mantissa, exponent)) = value.split_once('e') {
        let signed = exponent.trim_start_matches(['+', '-']);
        if signed.is_empty()
            || !signed.bytes().all(|c| c.is_ascii_digit())
            || exponent.len() - signed.len() > 1
        {
            return None;
        }
        (mantissa, exponent.parse::<i128>().ok()?)
    } else {
        (value, 0)
    };
    let mut digits = String::new(); // bounded: this one scalar's significant digits.
    let mut fractional = None;
    for ch in mantissa.chars() {
        if ch == '.' {
            if fractional.is_some() {
                return None;
            }
            fractional = Some(0usize);
        } else if ch.is_ascii_digit() {
            digits.push(ch);
            if let Some(count) = fractional.as_mut() {
                *count += 1;
            }
        } else {
            return None;
        }
    }
    if digits.is_empty() {
        return None;
    }
    let exponent = exponent.checked_sub(fractional.unwrap_or(0) as i128)?;
    if !(-1_999_999_999_999_999_997..=999_999_999_999_999_999).contains(&exponent) {
        return None;
    }
    let first = digits
        .bytes()
        .position(|c| c != b'0')
        .unwrap_or(digits.len());
    digits.drain(..first);
    Some(Parsed::Finite(Finite { digits, exponent }))
}

pub(super) fn decimal(text: &str, context: &DecimalContext) -> LexicalResult {
    if text.contains(['e', 'E']) {
        return LexicalResult::Diagnostic;
    }
    if parse(text).is_some() || !context.traps[INVALID] {
        LexicalResult::Valid
    } else {
        LexicalResult::Failure(LexicalFailure::DecimalInvalidOperation)
    }
}

fn rounding_up(rounding: Rounding, last: u8, first: u8, tail: bool, inexact: bool) -> bool {
    match rounding {
        Rounding::Ceiling | Rounding::Up => inexact,
        Rounding::Down | Rounding::Floor => false,
        Rounding::ZeroFiveUp => inexact && matches!(last, b'0' | b'5'),
        Rounding::HalfUp => first >= b'5',
        Rounding::HalfDown => first > b'5' || (first == b'5' && tail),
        Rounding::HalfEven => first > b'5' || (first == b'5' && (tail || (last - b'0') % 2 == 1)),
    }
}

impl Finite {
    fn adjusted(&self) -> i128 {
        self.exponent + self.digits.len() as i128 - 1
    }

    fn round(&mut self, context: &DecimalContext, signals: &mut [bool; 7]) {
        let etiny = i128::from(context.emin) - context.precision as i128 + 1;
        let discard =
            (self.digits.len() as i128 - context.precision as i128).max(etiny - self.exponent);
        if discard <= 0 {
            return;
        }
        signals[ROUNDED] = true;
        let keep = self.digits.len() as i128 - discard;
        let retained = keep.max(0) as usize;
        let bytes = self.digits.as_bytes();
        let discarded = &bytes[retained..];
        let inexact = discarded.iter().any(|&c| c != b'0');
        signals[INEXACT] = inexact;
        let first = if keep < 0 { b'0' } else { discarded[0] };
        let tail = if keep < 0 {
            inexact
        } else {
            discarded[1..].iter().any(|&c| c != b'0')
        };
        let last = retained.checked_sub(1).map_or(b'0', |index| bytes[index]);
        let up = rounding_up(context.rounding, last, first, tail, inexact);
        self.digits.truncate(retained);
        self.exponent += discard;
        if up {
            let mut digits = std::mem::take(&mut self.digits).into_bytes();
            let mut carry = true;
            for digit in digits.iter_mut().rev() {
                if *digit == b'9' {
                    *digit = b'0';
                } else {
                    *digit += 1;
                    carry = false;
                    break;
                }
            }
            if carry {
                digits.insert(0, b'1');
            }
            self.digits = String::from_utf8(digits).expect("ASCII decimal digits");
            if self.digits.len() > context.precision {
                self.digits.pop();
                self.exponent += 1;
            }
        }
    }

    fn above_power(&self, power: i128) -> bool {
        self.adjusted() > power
            || (self.adjusted() == power
                && (self.digits.as_bytes()[0] != b'1'
                    || self.digits.as_bytes()[1..].iter().any(|&c| c != b'0')))
    }
}

pub(super) fn float(text: &str, single: bool, context: &DecimalContext) -> bool {
    let Some(Parsed::Finite(mut value)) = parse(text) else {
        return false;
    };
    if value.digits.is_empty() {
        return false;
    }
    let mut signals = [false; 7]; // bounded: fixed Decimal signal vocabulary.
    signals[SUBNORMAL] = value.adjusted() < i128::from(context.emin);
    value.round(context, &mut signals);
    signals[UNDERFLOW] = signals[SUBNORMAL] && signals[INEXACT];
    if value.digits.is_empty() {
        signals[CLAMPED] = true;
    } else if value.adjusted() > i128::from(context.emax) {
        signals[OVERFLOW] = true;
        signals[INEXACT] = true;
        signals[ROUNDED] = true;
    } else if context.clamp
        && value.exponent > i128::from(context.emax) - context.precision as i128 + 1
    {
        signals[CLAMPED] = true;
    }
    if signals
        .into_iter()
        .zip(context.traps)
        .any(|(raised, trapped)| raised && trapped)
    {
        return false;
    }
    let (lower, upper) = if single {
        (-50i128, 40i128)
    } else {
        (-330i128, 310i128)
    };
    if signals[OVERFLOW] {
        return matches!(
            context.rounding,
            Rounding::Down | Rounding::Floor | Rounding::ZeroFiveUp
        ) && i128::from(context.emax) >= lower
            && i128::from(context.emax) < upper;
    }
    !value.digits.is_empty() && value.above_power(lower) && value.adjusted() < upper
}

/// Default RDFLib Decimal value comparison, distinct from Alt lexical diagnostics.
pub(super) fn zero(text: &str, context: &DecimalContext) -> Result<bool, LexicalFailure> {
    match parse(text) {
        Some(Parsed::Finite(value)) => Ok(value.digits.is_empty()),
        Some(Parsed::Nan { signaling: true }) if context.traps[INVALID] => {
            Err(LexicalFailure::DecimalInvalidComparison)
        }
        _ => Ok(false),
    }
}
