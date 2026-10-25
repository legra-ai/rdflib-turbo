//! Invocation-owned Core numeric roles. This is not the SPARQL comparison contract.
use std::cmp::Ordering;
use std::sync::Arc;

use bigdecimal::BigDecimal;
use oxigraph::model::Literal;

use crate::{Error, Result};

/// An exact finite scalar, including the full binary64 value rather than its display text.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FiniteNumber(BigDecimal);
impl FiniteNumber {
    /// Construct an arbitrary-precision coefficient times ten to an exponent.
    pub fn decimal(coefficient: &str, exponent: i64) -> Result<Self> {
        let value = format!("{coefficient}e{exponent}")
            .parse()
            .map_err(|error| Error::Invalid(format!("Invalid finite numeric operand: {error}")))?;
        Ok(Self(value))
    }
    /// Preserve the exact finite binary64 value, including subnormal values.
    pub fn float(value: f64) -> Result<Self> {
        BigDecimal::try_from(value)
            .map(Self)
            .map_err(|error| Error::Invalid(format!("Invalid finite binary64 operand: {error}")))
    }
}
/// The actual default numeric cache category, independently of storage precision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumericKind {
    Integer,
    Decimal,
    Float,
}
/// Parsed default numeric cache state. Failed conversions remain explicit missing values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NumericValue {
    Finite(FiniteNumber),
    NegativeInfinity,
    PositiveInfinity,
    QuietNaN,
    SignalingNaN,
    Missing,
}
/// One native comparison operand and its exact RDF identity for missing-value behavior.
pub struct NumericOperand<'a> {
    literal: &'a Literal,
    kind: NumericKind,
    value: NumericValue,
}
impl<'a> NumericOperand<'a> {
    /// Construct one operation-local operand without retaining Python objects.
    pub fn new(literal: &'a Literal, kind: NumericKind, value: NumericValue) -> Self {
        Self {
            literal,
            kind,
            value,
        }
    }
}
/// Core constraints invoke different Python comparison sequences.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComparisonRole {
    Range,
    PropertyPair,
}
/// A Core ordering operator, never a global SPARQL operator override.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComparisonOperator {
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}
impl ComparisonOperator {
    pub(crate) fn parse(operator: &str) -> Self {
        match operator {
            "<" => Self::Less,
            "<=" => Self::LessEqual,
            ">" => Self::Greater,
            ">=" => Self::GreaterEqual,
            _ => unreachable!("Core ordering operator"),
        }
    }
    fn allows(self, ordering: Ordering) -> bool {
        match self {
            Self::Less => ordering.is_lt(),
            Self::LessEqual => ordering.is_le(),
            Self::Greater => ordering.is_gt(),
            Self::GreaterEqual => ordering.is_ge(),
        }
    }
}
/// Caller-visible Decimal status signals raised by comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComparisonSignal {
    FloatOperation,
    InvalidOperation,
}
/// Per-invocation pure-native signal state. Implementations cannot call Python while comparing.
pub trait NumericComparisonContext: Send + Sync {
    /// Record a newly raised signal, without clearing existing caller status.
    fn record(&self, signal: ComparisonSignal);
    /// Return the trap snapshot captured before detached evaluation.
    fn traps(&self, signal: ComparisonSignal) -> bool;
    /// Consume NotImplemented in a boolean context using the captured host profile.
    /// The host records a warning only when this operation actually occurs.
    fn not_implemented_truth(&self) -> std::result::Result<bool, ComparisonFailure>;
}
/// Exact typed comparison failures; the Python bridge translates after native work ends.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ComparisonFailure {
    #[error("Decimal FloatOperation comparison")]
    FloatOperation,
    #[error("Decimal InvalidOperation comparison")]
    InvalidOperation,
    #[error("NotImplemented should not be used in a boolean context")]
    NotImplementedTruth,
    #[error("NotImplemented should not be used in a boolean context")]
    DeprecationWarning,
    #[error("'{operator}' not supported between instances of 'Literal' and 'Literal'")]
    UnsupportedOrdering { operator: &'static str },
    #[error(
        "I cannot know that these two lexical forms do not map to the same value: {left} and {right}"
    )]
    Incomparable { left: String, right: String },
}
/// Native host behavior for the complete recognized numeric family.
pub trait ValueComparisonPolicy: std::fmt::Debug + Send + Sync {
    /// Evaluate one Core role natively, including signal/trap semantics and missing conversions.
    fn compare_numeric(
        &self,
        role: ComparisonRole,
        operator: ComparisonOperator,
        left: &Literal,
        right: &Literal,
    ) -> std::result::Result<bool, ComparisonFailure>;
}
/// Select the authority explicitly. Python callers must always install Host.
#[derive(Clone, Debug, Default)]
pub enum CoreComparisonPolicy {
    /// Existing standalone Rust/SPARQL behavior, independently of a Python runtime.
    #[default]
    NativeSparql,
    /// Per-validation Python semantic authority, evaluated entirely in Rust.
    Host(Arc<dyn ValueComparisonPolicy>),
}
/// Recognize every supported XSD integer derivation, decimal and Python binary64 category.
pub fn numeric_kind(literal: &Literal) -> Option<NumericKind> {
    match literal
        .datatype()
        .as_str()
        .strip_prefix("http://www.w3.org/2001/XMLSchema#")?
    {
        "decimal" => Some(NumericKind::Decimal),
        "float" | "double" => Some(NumericKind::Float),
        "integer" | "byte" | "short" | "int" | "long" | "unsignedByte" | "unsignedShort"
        | "unsignedInt" | "unsignedLong" | "positiveInteger" | "negativeInteger"
        | "nonPositiveInteger" | "nonNegativeInteger" => Some(NumericKind::Integer),
        _ => None,
    }
}
fn mixed_float(left: &NumericOperand<'_>, right: &NumericOperand<'_>) -> bool {
    matches!(
        (left.kind, right.kind),
        (NumericKind::Decimal, NumericKind::Float) | (NumericKind::Float, NumericKind::Decimal)
    )
}
fn signal(
    context: &dyn NumericComparisonContext,
    signal: ComparisonSignal,
    trap: bool,
) -> std::result::Result<(), ComparisonFailure> {
    context.record(signal);
    if trap && context.traps(signal) {
        return Err(match signal {
            ComparisonSignal::FloatOperation => ComparisonFailure::FloatOperation,
            ComparisonSignal::InvalidOperation => ComparisonFailure::InvalidOperation,
        });
    }
    Ok(())
}
fn raw_compare(
    left: &NumericOperand<'_>,
    right: &NumericOperand<'_>,
    context: &dyn NumericComparisonContext,
    equality: bool,
) -> std::result::Result<Option<Ordering>, ComparisonFailure> {
    use NumericValue::*;
    if mixed_float(left, right) {
        signal(context, ComparisonSignal::FloatOperation, !equality)?;
    }
    let signaling = matches!(left.value, SignalingNaN) || matches!(right.value, SignalingNaN);
    let quiet = matches!(left.value, QuietNaN) || matches!(right.value, QuietNaN);
    if signaling || quiet {
        if signaling
            || (!equality
                && (left.kind == NumericKind::Decimal || right.kind == NumericKind::Decimal))
        {
            signal(context, ComparisonSignal::InvalidOperation, true)?;
        }
        return Ok(None);
    }
    Ok(Some(match (&left.value, &right.value) {
        (Finite(left), Finite(right)) => left.cmp(right),
        (NegativeInfinity, NegativeInfinity) | (PositiveInfinity, PositiveInfinity) => {
            Ordering::Equal
        }
        (NegativeInfinity, _) | (_, PositiveInfinity) => Ordering::Less,
        (PositiveInfinity, _) | (_, NegativeInfinity) => Ordering::Greater,
        (Missing, _) | (_, Missing) => {
            unreachable!("Missing values use literal metadata, never raw numeric comparison")
        }
        _ => unreachable!("NaN handled before ordered values"),
    }))
}
fn present(left: &NumericOperand<'_>, right: &NumericOperand<'_>) -> bool {
    !matches!(left.value, NumericValue::Missing) && !matches!(right.value, NumericValue::Missing)
}
fn equal(
    left: &NumericOperand<'_>,
    right: &NumericOperand<'_>,
    context: &dyn NumericComparisonContext,
) -> std::result::Result<bool, ComparisonFailure> {
    if present(left, right) {
        return Ok(raw_compare(left, right, context, true)? == Some(Ordering::Equal));
    }
    if left.literal.datatype() != right.literal.datatype() {
        return Ok(false);
    }
    if left.literal.value() == right.literal.value() {
        return Ok(true);
    }
    Err(ComparisonFailure::Incomparable {
        left: left.literal.value().to_owned(),
        right: right.literal.value().to_owned(),
    })
}
fn greater(
    left: &NumericOperand<'_>,
    right: &NumericOperand<'_>,
    context: &dyn NumericComparisonContext,
) -> std::result::Result<bool, ComparisonFailure> {
    if present(left, right) {
        return Ok(raw_compare(left, right, context, false)? == Some(Ordering::Greater));
    }
    Ok(if left.literal.datatype() != right.literal.datatype() {
        left.literal.datatype().as_str() > right.literal.datatype().as_str()
    } else {
        left.literal.value() > right.literal.value()
    })
}
/// Execute the actual Core numeric comparison sequence, including role-local error handling.
pub fn compare_numeric(
    role: ComparisonRole,
    operator: ComparisonOperator,
    left: NumericOperand<'_>,
    right: NumericOperand<'_>,
    context: &dyn NumericComparisonContext,
) -> std::result::Result<bool, ComparisonFailure> {
    if role == ComparisonRole::Range {
        let result = (|| {
            let ordering = if equal(&left, &right, context)? {
                Ordering::Equal
            } else if greater(&left, &right, context)? {
                Ordering::Greater
            } else {
                Ordering::Less
            };
            Ok(operator.allows(ordering))
        })();
        return match result {
            Err(ComparisonFailure::FloatOperation | ComparisonFailure::Incomparable { .. }) => {
                Ok(false)
            }
            result => result,
        };
    }
    let less = match (|| Ok(!greater(&left, &right, context)? && !equal(&left, &right, context)?))()
    {
        Err(ComparisonFailure::FloatOperation | ComparisonFailure::Incomparable { .. }) => None,
        Err(error) => return Err(error),
        Ok(value) => Some(value),
    };
    match operator {
        ComparisonOperator::Less => match less {
            Some(value) => Ok(value),
            // Literal.__lt__ returns NotImplemented after TypeError; Python then
            // invokes the right operand's reflected __gt__.
            None => greater(&right, &left, context),
        },
        ComparisonOperator::LessEqual => {
            let truth = match less {
                Some(value) => value,
                None => context.not_implemented_truth()?,
            };
            if truth {
                return Ok(true);
            }
            match equal(&left, &right, context) {
                Ok(value) => Ok(value),
                Err(ComparisonFailure::FloatOperation | ComparisonFailure::Incomparable { .. }) => {
                    if greater(&right, &left, context)? {
                        return Ok(true);
                    }
                    match equal(&right, &left, context) {
                        Err(
                            ComparisonFailure::FloatOperation
                            | ComparisonFailure::Incomparable { .. },
                        ) => Err(ComparisonFailure::UnsupportedOrdering { operator: "<=" }),
                        result => result,
                    }
                }
                Err(error) => Err(error),
            }
        }
        _ => unreachable!("Property-pair Core role only uses less than or less than or equal"),
    }
}
