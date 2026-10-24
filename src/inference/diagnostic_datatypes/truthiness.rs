//! Default RDFLib scalar truth values, separate from Alt lexical validation.

use oxigraph::model::Literal;

use super::{LexicalContext, cardinality, decimal, duration_truthiness, integer};

pub(in crate::inference) fn truthy(literal: &Literal, context: &LexicalContext) -> bool {
    let text = literal.value();
    if text.is_empty() {
        return false;
    }
    let datatype = literal.datatype();
    let Some(kind) = datatype
        .as_str()
        .strip_prefix("http://www.w3.org/2001/XMLSchema#")
    else {
        return true;
    };
    match kind {
        "boolean" => matches!(text.to_ascii_lowercase().as_str(), "true" | "1"),
        "integer" | "byte" | "short" | "int" | "long" | "unsignedByte" | "unsignedShort"
        | "unsignedInt" | "unsignedLong" | "positiveInteger" | "negativeInteger"
        | "nonPositiveInteger" | "nonNegativeInteger" => {
            !integer::digits(text, 10, context.integer_digit_limit)
                .is_some_and(|(_, digits)| digits.bytes().all(|digit| digit == b'0'))
        }
        // bool(Decimal) does not compare sNaN or raise comparison flags.
        "decimal" => !decimal::value_is_zero(text),
        "float" | "double" => !cardinality::float_zero(text),
        // The default decoder discards non-alphabet ASCII. With no data digits
        // it yields empty bytes; non-ASCII instead leaves a nonempty ill-typed term.
        "base64Binary" => {
            !text.is_ascii()
                || text
                    .bytes()
                    .any(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/'))
        }
        "duration" | "dayTimeDuration" | "yearMonthDuration" => !duration_truthiness::zero(text),
        // Successful temporal/XML values are objects with true truth values.
        // Strings retain their exact scalar text. Failed conversions use lexical
        // length, as Literal.__bool__ specifies, rather than Alt diagnostics.
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxigraph::model::NamedNode;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Case {
        datatype: String,
        lexical: String,
        truthy: bool,
    }

    #[test]
    fn default_value_truthiness_matches_pinned_rdflib() {
        let context = LexicalContext::default();
        for line in include_str!("truthiness-reference.jsonl").lines() {
            let case: Case = serde_json::from_str(line).unwrap();
            let literal =
                Literal::new_typed_literal(&case.lexical, NamedNode::new(&case.datatype).unwrap());
            assert_eq!(
                truthy(&literal, &context),
                case.truthy,
                "{} {:?}",
                case.datatype,
                case.lexical
            );
        }
        assert_eq!(
            context.decimal.signals.snapshot(),
            0,
            "truth checks must not compare signaling NaNs or raise Decimal flags"
        );
    }
}
