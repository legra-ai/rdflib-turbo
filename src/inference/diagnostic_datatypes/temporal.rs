use super::{LexicalContext, LexicalFailure, LexicalResult, unicode};

fn number(text: &str) -> Option<u32> {
    text.chars().try_fold(0u32, |value, ch| {
        value
            .checked_mul(10)?
            .checked_add(unicode::decimal(ch)?.into())
    })
}

fn two(text: &str, kind: char, hour_space: bool) -> Option<u32> {
    let mut chars = text.chars();
    let a = chars.next()?;
    let b = chars.next();
    if chars.next().is_some() {
        return None;
    }
    let valid = match (kind, b) {
        ('m', None) | ('d', None) => ('1'..='9').contains(&a),
        ('m', Some(b)) => {
            (a == '0' && ('1'..='9').contains(&b)) || (a == '1' && ('0'..='2').contains(&b))
        }
        ('d', Some(b)) => {
            (matches!(a, '1' | '2') && unicode::decimal(b).is_some())
                || (a == '3' && matches!(b, '0' | '1'))
                || (matches!(a, '0' | ' ') && ('1'..='9').contains(&b))
        }
        ('H', None) | ('M', None) | ('S', None) => unicode::decimal(a).is_some(),
        ('H', Some(b)) => {
            ((a == '0' || a == '1' || (a == ' ' && hour_space)) && unicode::decimal(b).is_some())
                || (a == '2' && ('0'..='3').contains(&b))
        }
        ('M', Some(b)) | ('S', Some(b)) => {
            ('0'..='5').contains(&a) && unicode::decimal(b).is_some()
        }
        _ => false,
    };
    valid
        .then(|| number(text.trim_start_matches(' ')))
        .flatten()
}

fn date(text: &str) -> bool {
    let mut parts = text.split('-');
    let (Some(year), Some(month), Some(day), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    if year.chars().count() != 4 {
        return false;
    }
    let (Some(year), Some(month), Some(day)) =
        (number(year), two(month, 'm', false), two(day, 'd', false))
    else {
        return false;
    };
    if year == 0 || month == 0 || month > 12 {
        return false;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max = match month {
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    day != 0 && day <= max
}

fn time(text: &str, context: &LexicalContext) -> bool {
    let mut parts = text.split(':');
    let (Some(hour), Some(minute), Some(second), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    two(hour, 'H', context.hour_leading_space).is_some()
        && two(minute, 'M', false).is_some()
        && two(second, 'S', false).is_some()
}

fn timezone(text: &str) -> (&str, bool) {
    if let Some(value) = text.strip_suffix('Z') {
        return (value, true);
    }
    let prefix = text.split('\n').next().unwrap_or("");
    let bytes = prefix.as_bytes();
    let found = bytes.windows(6).any(|slice| {
        matches!(slice[0], b'+' | b'-')
            && slice[1..3].iter().all(u8::is_ascii_digit)
            && slice[3] == b':'
            && slice[4..6].iter().all(u8::is_ascii_digit)
    });
    if found {
        let cut = text
            .char_indices()
            .rev()
            .nth(5)
            .map_or(0, |(index, _)| index);
        (&text[..cut], true)
    } else {
        (text, false)
    }
}

pub(super) fn check(text: &str, kind: &str, context: &LexicalContext) -> LexicalResult {
    let valid = match kind {
        "gYear" => date(&format!("{text}-01-01")),
        "gYearMonth" => date(&format!("{text}-01")),
        "gMonthDay" => date(&format!("2008-{text}")),
        "gDay" => date(&format!("2001-01-{text}")),
        "gMonth" => date(&format!("2001-{text}-01")),
        _ => {
            if text.is_empty() {
                return LexicalResult::Failure(LexicalFailure::IndexError);
            }
            let (value, zone) = timezone(text);
            if kind == "date" {
                date(value)
            } else {
                if kind == "dateTimeStamp" && !zone {
                    return LexicalResult::Diagnostic;
                }
                let prefix = value.split('\n').next().unwrap_or("");
                let value = if let Some(dot) = prefix.rfind('.') {
                    let fraction = &value[dot + 1..];
                    let end = fraction
                        .find(|ch: char| !ch.is_ascii_digit())
                        .unwrap_or(fraction.len());
                    let digits = &fraction[..end];
                    if digits.is_empty()
                        || (context.integer_digit_limit != 0
                            && digits.len() > context.integer_digit_limit)
                        || digits.parse::<u64>().ok().is_none_or(|n| n > 999999)
                    {
                        return LexicalResult::Diagnostic;
                    }
                    &value[..dot]
                } else {
                    value
                };
                if kind == "time" {
                    time(value, context)
                } else {
                    value
                        .split_once(['T', 't'])
                        .is_some_and(|(day, clock)| date(day) && time(clock, context))
                }
            }
        }
    };
    if valid {
        LexicalResult::Valid
    } else {
        LexicalResult::Diagnostic
    }
}
