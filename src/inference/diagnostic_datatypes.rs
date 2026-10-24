//! Diagnostic conversion follows owlrl 7.6.2, separately from RDFLib values.

use oxigraph::model::Literal;

mod cardinality;
mod context;
mod decimal;
mod duration_truthiness;
mod integer;
mod numeric;
mod signals;
mod temporal;
mod text;
mod truthiness;
mod unicode;

pub(crate) use context::LexicalContext;
pub(super) use numeric::{PythonNumericValue, parse_numeric};
pub(crate) use signals::SignalPublisher;
pub(super) use truthiness::truthy;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum LexicalResult {
    Valid,
    Diagnostic,
    Failure(LexicalFailure),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LexicalFailure {
    DecimalInvalidOperation,
    DecimalInvalidComparison,
    IndexError,
}

pub(super) fn check(literal: &Literal, context: &LexicalContext) -> LexicalResult {
    let iri = literal.datatype();
    let kind = if let Some(kind) = iri
        .as_str()
        .strip_prefix("http://www.w3.org/2001/XMLSchema#")
    {
        if matches!(kind, "XMLLiteral" | "PlainLiteral" | "HTML") {
            return LexicalResult::Valid;
        }
        kind
    } else if let Some(kind) = iri
        .as_str()
        .strip_prefix("http://www.w3.org/1999/02/22-rdf-syntax-ns#")
    {
        if !matches!(kind, "XMLLiteral" | "PlainLiteral" | "HTML") {
            return LexicalResult::Valid;
        }
        kind
    } else {
        return LexicalResult::Valid;
    };
    let value = literal.value();
    let valid = match kind {
        "integer" | "byte" | "short" | "int" | "long" | "unsignedByte" | "unsignedShort"
        | "unsignedInt" | "unsignedLong" | "positiveInteger" | "negativeInteger"
        | "nonPositiveInteger" | "nonNegativeInteger" => {
            integer::bounded(value, kind, context.integer_digit_limit)
        }
        "decimal" => return decimal::decimal(value, &context.decimal),
        "float" | "double" => decimal::float(value, kind == "float", &context.decimal),
        "boolean" => matches!(
            value.to_ascii_lowercase().as_str(),
            "true" | "false" | "0" | "1"
        ),
        "hexBinary" => integer::digits(value, 16, 0).is_some(),
        "base64Binary" => text::base64(value),
        "anyURI" => text::any_uri(value),
        "language" => text::language(value),
        "NMTOKEN" | "Name" | "NCName" => text::name(value, kind),
        "normalizedString" => text::normalized(value),
        "token" => text::token(value),
        "PlainLiteral" => text::plain(value),
        "XMLLiteral" => text::xml(value),
        "dateTime" | "dateTimeStamp" | "date" | "time" | "gYear" | "gMonth" | "gDay"
        | "gMonthDay" | "gYearMonth" => return temporal::check(value, kind, context),
        _ => true,
    };
    if valid {
        LexicalResult::Valid
    } else {
        LexicalResult::Diagnostic
    }
}

#[cfg(test)]
#[path = "diagnostic_datatypes/tests.rs"]
mod tests;

pub(super) fn cardinality_zero(
    literal: &Literal,
    context: &LexicalContext,
) -> Result<bool, LexicalFailure> {
    cardinality::zero(literal, context)
}

#[cfg(test)]
#[path = "diagnostic_datatypes/signal_tests.rs"]
mod signal_tests;
