//! Stock Python numeric conversions using the engine's existing Unicode policy.
use crate::constraints::DecimalDigitPolicy;

pub(super) fn float_zero(text: &str, digits: DecimalDigitPolicy) -> bool {
    let mut value = String::new(); // bounded: one scalar lexical value.
    let mut chars = text.trim().chars().peekable();
    let mut previous_digit = false;
    while let Some(ch) = chars.next() {
        if let Some(digit) = digits.decimal(ch) {
            value.push(char::from(b'0' + digit));
            previous_digit = true;
        } else if ch == '_' {
            if !previous_digit
                || !chars
                    .peek()
                    .is_some_and(|&next| digits.decimal(next).is_some())
            {
                return false;
            }
            previous_digit = false;
        } else {
            value.push(ch);
            previous_digit = false;
        }
    }
    value.parse::<f64>().is_ok_and(|number| number == 0.0)
}

pub(super) fn decimal_truthy(text: &str, digits: DecimalDigitPolicy) -> Option<bool> {
    let mut normalized = String::new(); // bounded: one scalar lexical value.
    for ch in text
        .trim_matches(|ch: char| ch.is_whitespace() || matches!(ch, '\u{1c}'..='\u{1f}'))
        .chars()
        .filter(|&ch| ch != '_')
    {
        if let Some(digit) = digits.decimal(ch) {
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
        return Some(true);
    }
    if let Some(payload) = value
        .strip_prefix("snan")
        .or_else(|| value.strip_prefix("nan"))
    {
        return payload
            .bytes()
            .all(|ch| ch.is_ascii_digit())
            .then_some(true);
    }
    let (mantissa, exponent) = if let Some((mantissa, exponent)) = value.split_once('e') {
        let signed = exponent.trim_start_matches(['+', '-']);
        if signed.is_empty()
            || !signed.bytes().all(|ch| ch.is_ascii_digit())
            || exponent.len() - signed.len() > 1
        {
            return None;
        }
        (mantissa, exponent.parse::<i128>().ok()?)
    } else {
        (value, 0)
    };
    let mut any_digit = false;
    let mut nonzero = false;
    let mut fractional = None;
    for ch in mantissa.chars() {
        if ch == '.' {
            if fractional.is_some() {
                return None;
            }
            fractional = Some(0usize);
        } else if ch.is_ascii_digit() {
            any_digit = true;
            nonzero |= ch != '0';
            if let Some(count) = fractional.as_mut() {
                *count += 1;
            }
        } else {
            return None;
        }
    }
    let exponent = exponent.checked_sub(fractional.unwrap_or(0) as i128)?;
    (any_digit && (-1_999_999_999_999_999_997..=999_999_999_999_999_999).contains(&exponent))
        .then_some(nonzero)
}
