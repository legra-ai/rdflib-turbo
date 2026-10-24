//! OWL cardinality compares default RDFLib Python values, not Alt conversions.

use super::{LexicalContext, LexicalFailure, decimal, integer};
use oxigraph::model::Literal;

pub(super) fn float_zero(text: &str) -> bool {
    integer::float_value(text).is_some_and(|number| number == 0.0)
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
