//! Default RDFLib node bool; conversion failure is lexical truth, not EBV.
use num_bigint::Sign;
use oxigraph::model::{Literal, Term};

use crate::constraints::{DecimalDigitPolicy, integer_lexical_with_policy};

#[path = "constraint_list_duration.rs"]
mod duration;
#[path = "constraint_list_numbers.rs"]
mod numbers;

#[cfg(test)]
pub(super) fn truthy(term: &Term) -> bool {
    truthy_with_policy(term, Default::default())
}

pub(super) fn truthy_with_policy(term: &Term, digits: DecimalDigitPolicy) -> bool {
    match term {
        Term::NamedNode(node) => !node.as_str().is_empty(),
        Term::BlankNode(node) => !node.as_str().is_empty(),
        Term::Literal(literal) => literal_truthy(literal, digits),
    }
}

fn literal_truthy(literal: &Literal, digits: DecimalDigitPolicy) -> bool {
    let value = literal.value();
    if value.is_empty() {
        return false;
    }
    let datatype = literal.datatype();
    let Some(kind) = datatype
        .as_str()
        .strip_prefix("http://www.w3.org/2001/XMLSchema#")
    else {
        // Stock RDFLib has no rdf:JSON converter. Custom bindings are separate.
        return true;
    };
    match kind {
        "boolean" => matches!(value.to_ascii_lowercase().as_str(), "true" | "1"),
        "integer" | "byte" | "short" | "int" | "long" | "unsignedByte" | "unsignedShort"
        | "unsignedInt" | "unsignedLong" | "positiveInteger" | "negativeInteger"
        | "nonPositiveInteger" | "nonNegativeInteger" => integer_lexical_with_policy(value, digits)
            .is_none_or(|number| number.sign() != Sign::NoSign),
        "decimal" => numbers::decimal_truthy(value, digits).unwrap_or(true),
        "float" | "double" => !numbers::float_zero(value, digits),
        "base64Binary" => {
            // No alphabet yields empty decoded bytes. Alphabet input yields
            // bytes or a failed conversion (value=None), both truthy here.
            !value.is_ascii()
                || value
                    .bytes()
                    .any(|c| c.is_ascii_alphanumeric() || c == b'+' || c == b'/')
        }
        "duration" | "dayTimeDuration" | "yearMonthDuration" => duration::truthy(value),
        _ => true,
    }
}
