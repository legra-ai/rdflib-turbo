//! Stock CPython float-string classification used only by Literal.n3 numeric output.
use std::borrow::Cow;

include!(concat!(env!("OUT_DIR"), "/python_scalar_imports.rs"));

pub(super) enum Number {
    Finite,
    Infinite,
    Nan,
    Invalid,
}

pub(super) fn classify(source: &str) -> Number {
    let normalized: Cow<'_, str> = if source.is_ascii() {
        Cow::Borrowed(source)
    } else {
        Cow::Owned(
            source
                .chars()
                .map(|ch| {
                    if ch.is_ascii() {
                        return ch;
                    }
                    if let Some(digit) = shacl_bindings::python_decimal_digit(ch) {
                        return char::from(b'0' + digit);
                    }
                    // SAFETY: immutable CPython scalar table; no object, GIL or interpreter state.
                    if unsafe { _PyUnicode_IsWhitespace(ch as u32) != 0 } {
                        ' '
                    } else {
                        ch
                    }
                })
                .collect(),
        )
    };
    let value = normalized.trim_matches([' ', '\t', '\r', '\n', '\x0b', '\x0c']);
    let bytes = value.as_bytes();
    if bytes.iter().enumerate().any(|(i, byte)| {
        *byte == b'_'
            && (i == 0
                || i + 1 == bytes.len()
                || !bytes[i - 1].is_ascii_digit()
                || !bytes[i + 1].is_ascii_digit())
    }) {
        return Number::Invalid;
    }
    let cleaned = if value.contains('_') {
        Cow::Owned(value.replace('_', ""))
    } else {
        Cow::Borrowed(value)
    };
    match cleaned.parse::<f64>() {
        Ok(value) if value.is_infinite() => Number::Infinite,
        Ok(value) if value.is_nan() => Number::Nan,
        Ok(_) => Number::Finite,
        Err(_) => Number::Invalid,
    }
}

#[cfg(test)]
mod tests {
    use super::{Number, classify};

    #[test]
    fn matches_cpython_float_string_classification_without_cached_literal_values() {
        pyo3::Python::initialize();
        for (value, expected) in [
            ("+infinity", "infinite"),
            ("Inf", "infinite"),
            ("Infinity", "infinite"),
            ("NAN", "nan"),
            ("nan(payload)", "invalid"),
            (" ١_٢ ", "finite"),
            ("\u{85}Infinity\u{85}", "infinite"),
            ("\u{1c}1", "invalid"),
            ("1__2", "invalid"),
            ("1_2", "finite"),
            ("1e999", "infinite"),
            ("-nan", "nan"),
            (".5", "finite"),
            ("1.", "finite"),
            ("0x1", "invalid"),
        ] {
            let actual = match classify(value) {
                Number::Finite => "finite",
                Number::Infinite => "infinite",
                Number::Nan => "nan",
                Number::Invalid => "invalid",
            };
            assert_eq!(actual, expected, "CPython float({value:?})");
        }
    }
}
