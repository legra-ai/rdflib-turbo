//! Canonical numeric Core comparison policy over captured lexical context.
use oxigraph::model::Literal;
use pyshacl_turbo_engine::{
    ComparisonFailure, ComparisonOperator, ComparisonRole, ComparisonSignal, FiniteNumber,
    NumericComparisonContext, NumericKind, NumericOperand, NumericValue, ValueComparisonPolicy,
    compare_numeric, numeric_kind,
};

use super::super::{
    LexicalContext, comparison_warning,
    decimal::{self, DecimalParts},
    integer,
};

#[derive(Debug)]
pub(super) struct Policy {
    pub(super) context: LexicalContext,
    pub(super) warning: comparison_warning::Profile,
}
impl NumericComparisonContext for Policy {
    fn record(&self, signal: ComparisonSignal) {
        match signal {
            ComparisonSignal::FloatOperation => self.context.record_float_operation(),
            ComparisonSignal::InvalidOperation => self.context.record_invalid_operation(),
        }
    }
    fn traps(&self, signal: ComparisonSignal) -> bool {
        match signal {
            ComparisonSignal::FloatOperation => self.context.float_operation_trap,
            ComparisonSignal::InvalidOperation => self.context.traps_invalid_operation(),
        }
    }
    fn not_implemented_truth(&self) -> Result<bool, ComparisonFailure> {
        self.warning.consume()
    }
}
impl Policy {
    fn parse<'a>(&self, literal: &'a Literal) -> Result<NumericOperand<'a>, ComparisonFailure> {
        let kind = numeric_kind(literal).expect("Core typed numeric route");
        let value = match kind {
            NumericKind::Integer => {
                match integer::digits(literal.value(), 10, self.context.integer_digit_limit) {
                    Some((negative, digits)) => NumericValue::Finite(
                        FiniteNumber::decimal(
                            &format!("{}{digits}", if negative { "-" } else { "" }),
                            0,
                        )
                        .expect("validated native integer"),
                    ),
                    None => NumericValue::Missing,
                }
            }
            NumericKind::Decimal => match decimal::parts(literal.value()) {
                Some(DecimalParts::Finite {
                    negative,
                    digits,
                    exponent,
                }) => NumericValue::Finite(
                    FiniteNumber::decimal(
                        &format!("{}{digits}", if negative { "-" } else { "" }),
                        i64::try_from(exponent).expect("CPython Decimal exponent fits i64"),
                    )
                    .expect("validated native decimal"),
                ),
                Some(DecimalParts::Infinity { negative }) => {
                    if negative {
                        NumericValue::NegativeInfinity
                    } else {
                        NumericValue::PositiveInfinity
                    }
                }
                Some(DecimalParts::NaN { signaling }) => {
                    if signaling {
                        NumericValue::SignalingNaN
                    } else {
                        NumericValue::QuietNaN
                    }
                }
                None => NumericValue::Missing,
            },
            NumericKind::Float => match integer::float_value(literal.value()) {
                Some(value) if value.is_nan() => NumericValue::QuietNaN,
                Some(value) if value.is_infinite() => {
                    if value.is_sign_negative() {
                        NumericValue::NegativeInfinity
                    } else {
                        NumericValue::PositiveInfinity
                    }
                }
                Some(value) => {
                    NumericValue::Finite(FiniteNumber::float(value).expect("finite binary64"))
                }
                None => NumericValue::Missing,
            },
        };
        Ok(NumericOperand::new(literal, kind, value))
    }
}
impl ValueComparisonPolicy for Policy {
    fn compare_numeric(
        &self,
        role: ComparisonRole,
        operator: ComparisonOperator,
        left: &Literal,
        right: &Literal,
    ) -> Result<bool, ComparisonFailure> {
        compare_numeric(role, operator, self.parse(left)?, self.parse(right)?, self)
    }
}
