pub(crate) mod comparison;

use num_bigint::BigInt;
use oxigraph::model::*;
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults};

use crate::graph::*;
use crate::{Error, Result};

#[cfg(test)]
#[path = "value_tests.rs"]
mod tests;

pub(crate) fn lexical(term: &Term) -> Option<&str> {
    match term {
        Term::NamedNode(n) => Some(n.as_str()),
        Term::Literal(l) => Some(l.value()),
        _ => None,
    }
}

pub(crate) fn expression(
    query: &PreparedSparqlQuery,
    data: &(impl crate::GraphRead + ?Sized),
    value: &Term,
    other: Option<&Term>,
) -> Result<bool> {
    let mut evaluation = query
        .clone()
        .on_queryable_dataset(crate::QueryGraph::new(data))
        .substitute_variable(Variable::new_unchecked("value"), value.clone());
    if let Some(other) = other {
        evaluation =
            evaluation.substitute_variable(Variable::new_unchecked("other"), other.clone());
    }
    let QueryResults::Solutions(mut solutions) = evaluation.execute()? else {
        return Err(Error::Runtime(
            "Constraint expression must return a solution".into(),
        ));
    };
    let solution = solutions
        .next()
        .ok_or_else(|| Error::Runtime("Constraint expression returned no row".into()))??;
    // SPARQL expression errors (e.g. incomparable operands) produce an unbound
    // result. SHACL bounds and property-pair comparisons require a violation.
    Ok(solution.get("ok") == Some(&Literal::from(true).into()))
}

pub(crate) fn datatype(value: &Term, datatype: &NamedNode) -> bool {
    let Term::Literal(value) = value else {
        return false;
    };
    if datatype == &iri(RDFS, "Literal") || datatype == &iri(RDFS, "Datatype") {
        return true;
    }
    if value.datatype() != datatype.as_ref() {
        return false;
    }
    let Some(kind) = datatype.as_str().strip_prefix(XSD) else {
        return true;
    };
    let text = value.value();
    match kind {
        "boolean" => matches!(text, "true" | "false" | "0" | "1"),
        "integer" | "long" | "int" | "short" | "byte" | "unsignedLong" | "unsignedInt"
        | "unsignedShort" | "unsignedByte" | "nonNegativeInteger" | "positiveInteger"
        | "nonPositiveInteger" | "negativeInteger" => {
            let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return false;
            }
            let Ok(number) = text.parse::<BigInt>() else {
                return false;
            };
            let (min, max): (Option<BigInt>, Option<BigInt>) = match kind {
                "long" => (Some(i64::MIN.into()), Some(i64::MAX.into())),
                "int" => (Some(i32::MIN.into()), Some(i32::MAX.into())),
                "short" => (Some(i16::MIN.into()), Some(i16::MAX.into())),
                "byte" => (Some(i8::MIN.into()), Some(i8::MAX.into())),
                "unsignedLong" => (Some(0.into()), Some(u64::MAX.into())),
                "unsignedInt" => (Some(0.into()), Some(u32::MAX.into())),
                "unsignedShort" => (Some(0.into()), Some(u16::MAX.into())),
                "unsignedByte" => (Some(0.into()), Some(u8::MAX.into())),
                "nonNegativeInteger" => (Some(0.into()), None),
                "positiveInteger" => (Some(1.into()), None),
                "nonPositiveInteger" => (None, Some(0.into())),
                "negativeInteger" => (None, Some((-1).into())),
                _ => (None, None),
            };
            min.is_none_or(|n| number >= n) && max.is_none_or(|n| number <= n)
        }
        "decimal" => {
            let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
            let mut dots = 0;
            let mut count = 0;
            for c in digits.bytes() {
                if c == b'.' {
                    dots += 1;
                } else if c.is_ascii_digit() {
                    count += 1;
                } else {
                    return false;
                }
            }
            dots <= 1 && count > 0
        }
        "float" => text.parse::<oxsdatatypes::Float>().is_ok(),
        "double" => text.parse::<oxsdatatypes::Double>().is_ok(),
        "date" => text.parse::<oxsdatatypes::Date>().is_ok(),
        "time" => text.parse::<oxsdatatypes::Time>().is_ok(),
        "dateTime" => text.parse::<oxsdatatypes::DateTime>().is_ok(),
        "dateTimeStamp" => text
            .parse::<oxsdatatypes::DateTime>()
            .is_ok_and(|v| v.timezone().is_some()),
        "duration" => text.parse::<oxsdatatypes::Duration>().is_ok(),
        "dayTimeDuration" => text.parse::<oxsdatatypes::DayTimeDuration>().is_ok(),
        "yearMonthDuration" => text.parse::<oxsdatatypes::YearMonthDuration>().is_ok(),
        "gYear" => text.parse::<oxsdatatypes::GYear>().is_ok(),
        "gMonth" => text.parse::<oxsdatatypes::GMonth>().is_ok(),
        "gDay" => text.parse::<oxsdatatypes::GDay>().is_ok(),
        "gYearMonth" => text.parse::<oxsdatatypes::GYearMonth>().is_ok(),
        "gMonthDay" => text.parse::<oxsdatatypes::GMonthDay>().is_ok(),
        "hexBinary" => text.len().is_multiple_of(2) && text.bytes().all(|c| c.is_ascii_hexdigit()),
        "string" | "anyURI" => true,
        "normalizedString" => !text.contains(['\r', '\n', '\t']),
        "token" => {
            !text.contains(['\r', '\n', '\t']) && !text.contains("  ") && text.trim() == text
        }
        _ => true,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn compare(
    policy: &crate::CoreComparisonPolicy,
    role: crate::ComparisonRole,
    operator: crate::ComparisonOperator,
    query: &PreparedSparqlQuery,
    data: &(impl crate::GraphRead + ?Sized),
    value: &Term,
    other: &Term,
) -> Result<bool> {
    if let (crate::CoreComparisonPolicy::Host(policy), Term::Literal(left), Term::Literal(right)) =
        (policy, value, other)
        && crate::numeric_kind(left).is_some()
        && crate::numeric_kind(right).is_some()
    {
        return policy
            .compare_numeric(role, operator, left, right)
            .map_err(Error::Comparison);
    }
    expression(query, data, value, Some(other))
}
