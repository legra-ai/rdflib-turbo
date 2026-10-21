//! OWL cardinality compares default RDFLib Python values, not Alt conversions.

use super::{LexicalContext, LexicalFailure, decimal, integer, unicode};
use oxigraph::model::Literal;

fn float_zero(text: &str) -> bool {
    let mut value = String::new(); // bounded: this one scalar lexical value.
    let mut chars = text.trim_matches(integer::whitespace).chars().peekable();
    let mut previous_digit = false;
    while let Some(ch) = chars.next() {
        if let Some(digit) = unicode::decimal(ch) {
            value.push(char::from(b'0' + digit));
            previous_digit = true;
        } else if ch == '_' {
            if !previous_digit
                || !chars
                    .peek()
                    .is_some_and(|&next| unicode::decimal(next).is_some())
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

pub(super) fn zero(literal: &Literal, context: &LexicalContext) -> Result<bool, LexicalFailure> {
    let iri = literal.datatype();
    let Some(kind) = iri
        .as_str()
        .strip_prefix("http://www.w3.org/2001/XMLSchema#")
    else {
        return Ok(false);
    };
    let value = literal.value();
    Ok(match kind {
        "boolean" => !matches!(value.to_ascii_lowercase().as_str(), "true" | "1"),
        "integer" | "byte" | "short" | "int" | "long" | "unsignedByte" | "unsignedShort"
        | "unsignedInt" | "unsignedLong" | "positiveInteger" | "negativeInteger"
        | "nonPositiveInteger" | "nonNegativeInteger" => {
            integer::digits(value, 10, context.integer_digit_limit)
                .is_some_and(|(_, digits)| digits.bytes().all(|c| c == b'0'))
        }
        "decimal" => return decimal::zero(value, &context.decimal),
        "float" | "double" => float_zero(value),
        _ => false,
    })
}
