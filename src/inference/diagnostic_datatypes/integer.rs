use super::unicode;

/// Python int lexical scanner; retained text is bounded by this one literal.
pub(super) fn digits(text: &str, radix: u8, limit: usize) -> Option<(bool, String)> {
    let mut text = text.trim_matches(whitespace);
    let negative = text.starts_with('-');
    if text.starts_with(['-', '+']) {
        text = &text[1..];
    }
    if radix == 16 && (text.starts_with("0x") || text.starts_with("0X")) {
        text = &text[2..];
        if text.starts_with('_') {
            text = &text[1..];
        }
    }
    let mut output = String::new(); // bounded: at most the scalar literal's byte length.
    let mut previous_digit = false;
    for ch in text.chars() {
        if ch == '_' {
            if !previous_digit {
                return None;
            }
            previous_digit = false;
            continue;
        }
        let digit = unicode::decimal(ch).or_else(|| ch.to_digit(radix.into()).map(|n| n as u8))?;
        if digit >= radix {
            return None;
        }
        output.push(char::from_digit(digit.into(), radix.into())?);
        previous_digit = true;
    }
    if !previous_digit || (radix == 10 && limit != 0 && output.len() > limit) {
        return None;
    }
    Some((negative, output))
}

pub(super) fn bounded(text: &str, kind: &str, limit: usize) -> bool {
    let Some((negative, digits)) = digits(text, 10, limit) else {
        return false;
    };
    let digits = digits.trim_start_matches('0');
    let negative = negative && !digits.is_empty();
    let bound = match kind {
        "byte" => {
            if negative {
                "128"
            } else {
                "127"
            }
        }
        "short" => {
            if negative {
                "32768"
            } else {
                "32767"
            }
        }
        "int" => {
            if negative {
                "2147483648"
            } else {
                "2147483647"
            }
        }
        "long" => {
            if negative {
                "9223372036854775808"
            } else {
                "9223372036854775807"
            }
        }
        "unsignedByte" => "255",
        "unsignedShort" => "65535",
        "unsignedInt" => "4294967295",
        "unsignedLong" => "18446744073709551615",
        "positiveInteger" => return !negative && !digits.is_empty(),
        "negativeInteger" => return negative,
        "nonPositiveInteger" => return negative || digits.is_empty(),
        "nonNegativeInteger" => return !negative,
        "integer" => return true,
        _ => return false,
    };
    (!kind.starts_with("unsigned") || !negative)
        && (digits.len() < bound.len() || (digits.len() == bound.len() && digits <= bound))
}

pub(super) fn whitespace(ch: char) -> bool {
    if ch.is_ascii() {
        matches!(ch, ' ' | '\t'..='\r')
    } else {
        unicode::whitespace(ch)
    }
}
