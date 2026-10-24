//! Default RDFLib numeric values; exact finite equality comes from rdf-reasoning.
#[cfg(test)]
use super::LexicalFailure;
use super::decimal::{self, DecimalParts};
use super::{LexicalContext, integer};
use oxigraph::model::Literal;
use rdf_reasoning::ExactNumber;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum KeyKind {
    Finite(ExactNumber),
    Infinity { negative: bool },
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(in crate::inference) struct NumericKey(KeyKind);
impl NumericKey {
    fn digits(negative: bool, digits: &str, exponent: i128) -> Self {
        let digits = if digits.is_empty() { "0" } else { digits };
        let coefficient = if negative {
            format!("-{digits}")
        } else {
            digits.to_owned()
        };
        // Decimal parser bounds its exponent below 2e18; normalization adds at
        // most this scalar's bit length, far below the signed i64 limit.
        Self(KeyKind::Finite(
            ExactNumber::try_from_decimal(
                &coefficient,
                i64::try_from(exponent).expect("CPython Decimal exponent fits i64"),
            )
            .expect("validated scalar coefficient and exponent"),
        ))
    }
    fn float(value: f64) -> Self {
        Self(KeyKind::Finite(
            ExactNumber::try_from_f64(value).expect("finite binary64"),
        ))
    }
    pub(in crate::inference) fn key(&self) -> String {
        match &self.0 {
            KeyKind::Finite(value) => format!("number:{value}"),
            KeyKind::Infinity { negative } => format!("infinity:{negative}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::inference) enum PythonNumericValue {
    Comparable(NumericKey),
    QuietNaN,
    SignalingNaN,
    Missing,
}

pub(in crate::inference) fn parse_numeric(
    literal: &Literal,
    context: &LexicalContext,
) -> Option<PythonNumericValue> {
    use PythonNumericValue::{Comparable, Missing, QuietNaN, SignalingNaN};
    let iri = literal.datatype();
    let kind = iri
        .as_str()
        .strip_prefix("http://www.w3.org/2001/XMLSchema#")?;
    let value = literal.value();
    Some(match kind {
        "boolean" => Comparable(NumericKey::digits(
            false,
            if matches!(value.to_ascii_lowercase().as_str(), "true" | "1") {
                "1"
            } else {
                "0"
            },
            0,
        )),
        "integer" | "byte" | "short" | "int" | "long" | "unsignedByte" | "unsignedShort"
        | "unsignedInt" | "unsignedLong" | "positiveInteger" | "negativeInteger"
        | "nonPositiveInteger" | "nonNegativeInteger" => {
            match integer::digits(value, 10, context.integer_digit_limit) {
                Some((negative, digits)) => Comparable(NumericKey::digits(negative, &digits, 0)),
                None => Missing,
            }
        }
        "decimal" => match decimal::parts(value) {
            Some(DecimalParts::Finite {
                negative,
                digits,
                exponent,
            }) => Comparable(NumericKey::digits(negative, &digits, exponent)),
            Some(DecimalParts::Infinity { negative }) => {
                Comparable(NumericKey(KeyKind::Infinity { negative }))
            }
            Some(DecimalParts::NaN { signaling: true }) => SignalingNaN,
            Some(DecimalParts::NaN { signaling: false }) => QuietNaN,
            None => {
                context.record_invalid_operation();
                if context.decimal.traps[0] {
                    Missing
                } else {
                    QuietNaN
                }
            }
        },
        "float" | "double" => match integer::float_value(value) {
            Some(number) if number.is_nan() => QuietNaN,
            Some(number) if number.is_infinite() => Comparable(NumericKey(KeyKind::Infinity {
                negative: number.is_sign_negative(),
            })),
            Some(number) => Comparable(NumericKey::float(number)),
            None => Missing,
        },
        _ => return None,
    })
}

#[cfg(test)]
pub(in crate::inference) fn numeric_equal(
    left: &PythonNumericValue,
    right: &PythonNumericValue,
    context: &LexicalContext,
) -> Result<Option<bool>, LexicalFailure> {
    use PythonNumericValue::{Comparable, Missing, SignalingNaN};
    if matches!(left, Missing) || matches!(right, Missing) {
        return Ok(None);
    }
    if (matches!(left, SignalingNaN) || matches!(right, SignalingNaN)) && context.decimal.traps[0] {
        return Err(LexicalFailure::DecimalInvalidComparison);
    }
    Ok(Some(
        matches!((left, right), (Comparable(a),Comparable(b)) if a == b),
    ))
}

#[cfg(test)]
#[path = "numeric_tests.rs"]
mod tests;
