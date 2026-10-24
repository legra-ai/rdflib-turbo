//! The zero timedelta subset of RDFLib's default ISO duration converter.

pub(super) fn zero(text: &str) -> bool {
    // The reference regex's final $ also accepts one final newline.
    let text = text.strip_suffix('\n').unwrap_or(text);
    let text = text.strip_prefix(['+', '-']).unwrap_or(text);
    let Some(mut rest) = text.strip_prefix('P') else {
        return false;
    };
    if rest.is_empty() {
        return false;
    }
    let mut time = false;
    let mut previous = 0;
    let mut components = false;
    let mut micros = 0.0_f64;
    while !rest.is_empty() {
        if let Some(tail) = rest.strip_prefix('T') {
            if time {
                return false;
            }
            time = true;
            previous = 4;
            rest = tail;
            continue;
        }
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return false;
        }
        let mut length = digits;
        if rest
            .as_bytes()
            .get(length)
            .is_some_and(|byte| matches!(byte, b'.' | b','))
        {
            length += 1;
            let fractional = rest[length..]
                .bytes()
                .take_while(u8::is_ascii_digit)
                .count();
            if fractional == 0 {
                return false;
            }
            length += fractional;
        }
        let Some(unit) = rest.as_bytes().get(length).copied() else {
            return false;
        };
        let rank = match (time, unit) {
            (false, b'Y') => 1,
            (false, b'M') => 2,
            (false, b'W') => 3,
            (false, b'D') => 4,
            (true, b'H') => 5,
            (true, b'M') => 6,
            (true, b'S') => 7,
            _ => return false,
        };
        if rank <= previous {
            return false;
        }
        previous = rank;
        components = true;
        let amount = &rest[..length];
        if rank <= 2 {
            // A nonzero year/month component returns Duration, whose default
            // object truth value is true, even below timedelta precision.
            if amount.bytes().any(|byte| matches!(byte, b'1'..=b'9')) {
                return false;
            }
        } else {
            // bounded: one scalar numeric component, never a graph collection.
            let amount = amount.replace(',', ".");
            let Ok(value) = amount.parse::<f64>() else {
                return false;
            };
            let factor = match rank {
                3 => 604_800_000_000.0,
                4 => 86_400_000_000.0,
                5 => 3_600_000_000.0,
                6 => 60_000_000.0,
                7 => 1_000_000.0,
                _ => unreachable!(),
            };
            micros += value * factor;
        }
        rest = &rest[length + 1..];
    }
    // timedelta rounds sub-microsecond totals to the nearest even microsecond.
    (components || time) && micros <= 0.5
}
