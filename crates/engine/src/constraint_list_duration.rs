//! The only false duration values are zero Python timedeltas after rounding.

pub(super) fn truthy(value: &str) -> bool {
    // RDFLib's anchored Python regex accepts one final newline via `$`.
    let value = value.strip_suffix('\n').unwrap_or(value);
    let value = if value.starts_with(['+', '-']) {
        &value[1..]
    } else {
        value
    };
    let Some(mut rest) = value.strip_prefix('P') else {
        return true;
    };
    if rest.is_empty() {
        return true;
    }
    let mut time = false;
    let mut previous = None;
    // bounded: Python timedelta's five scalar fields in accumulation order.
    let mut parts = [0.0_f64; 5];
    while !rest.is_empty() {
        if let Some(tail) = rest.strip_prefix('T') {
            if time {
                return true;
            }
            time = true;
            previous = None;
            rest = tail;
            continue;
        }
        let mut end = rest.bytes().take_while(u8::is_ascii_digit).count();
        if end == 0 {
            return true;
        }
        if matches!(rest.as_bytes().get(end), Some(b'.' | b',')) {
            end += 1;
            let digits = rest[end..].bytes().take_while(u8::is_ascii_digit).count();
            if digits == 0 {
                return true;
            }
            end += digits;
        }
        let number = &rest[..end];
        let Some(&unit) = rest.as_bytes().get(end) else {
            return true;
        };
        let order = match (time, unit) {
            (false, b'Y') => 0,
            (false, b'M') => 1,
            (false, b'W') => 2,
            (false, b'D') => 3,
            (true, b'H') => 0,
            (true, b'M') => 1,
            (true, b'S') => 2,
            _ => return true,
        };
        if previous.is_some_and(|previous| order <= previous) {
            return true;
        }
        previous = Some(order);
        if !time && matches!(unit, b'Y' | b'M') {
            // Nonzero years/months yield a Duration object (always truthy).
            if number.bytes().any(|c| c.is_ascii_digit() && c != b'0') {
                return true;
            }
        } else {
            let normalized = number.replace(',', "."); // bounded: one scalar field.
            let Ok(number) = normalized.parse::<f64>() else {
                return true;
            };
            let (slot, scale) = match unit {
                b'D' => (0, 86_400_000_000.0),
                b'S' => (1, 1_000_000.0),
                b'M' => (2, 60_000_000.0),
                b'H' => (3, 3_600_000_000.0),
                b'W' => (4, 604_800_000_000.0),
                _ => unreachable!(),
            };
            parts[slot] = number * scale;
        }
        rest = &rest[end + 1..];
    }
    let micros = parts.into_iter().sum::<f64>();
    // Overflow produces value=None; the nonempty lexical form remains truthy.
    !micros.is_finite() || micros.round_ties_even() != 0.0
}
