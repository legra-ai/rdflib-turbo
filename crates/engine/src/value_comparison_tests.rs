//! The Core numeric roles preserve Python comparison order, independently of SPARQL.
use std::sync::atomic::{AtomicU8, Ordering};

use super::super::comparison::{
    ComparisonFailure, ComparisonOperator, ComparisonRole, ComparisonSignal, FiniteNumber,
    NumericComparisonContext, NumericKind, NumericOperand, NumericValue, compare_numeric,
};
use oxigraph::model::{Literal, NamedNode};

#[derive(Default)]
struct Context {
    float_trap: bool,
    truth_error: bool,
    warning_error: bool,
    warnings: AtomicU8,
    invalid_trap: bool,
    flags: AtomicU8,
}
impl NumericComparisonContext for Context {
    fn not_implemented_truth(&self) -> Result<bool, ComparisonFailure> {
        if self.truth_error {
            return Err(ComparisonFailure::NotImplementedTruth);
        }
        self.warnings.fetch_add(1, Ordering::Relaxed);
        if self.warning_error {
            return Err(ComparisonFailure::DeprecationWarning);
        }
        Ok(true)
    }

    fn record(&self, signal: ComparisonSignal) {
        self.flags.fetch_or(1 << signal as u8, Ordering::Relaxed);
    }
    fn traps(&self, signal: ComparisonSignal) -> bool {
        match signal {
            ComparisonSignal::FloatOperation => self.float_trap,
            ComparisonSignal::InvalidOperation => self.invalid_trap,
        }
    }
}
fn literal(value: &str, datatype: &str) -> Literal {
    Literal::new_typed_literal(
        value,
        NamedNode::new_unchecked(format!("http://www.w3.org/2001/XMLSchema#{datatype}")),
    )
}
fn decimal(value: &str) -> NumericValue {
    NumericValue::Finite(FiniteNumber::decimal(value, 0).unwrap())
}
#[test]
fn range_equality_bypasses_float_trap_but_unequal_operands_do_not() {
    let floating = literal("1.0", "double");
    let equal = literal("1", "decimal");
    let lower = literal("0.9", "decimal");
    for operator in [
        ComparisonOperator::Less,
        ComparisonOperator::LessEqual,
        ComparisonOperator::Greater,
        ComparisonOperator::GreaterEqual,
    ] {
        let context = Context {
            float_trap: true,
            ..Context::default()
        };
        let actual = compare_numeric(
            ComparisonRole::Range,
            operator,
            NumericOperand::new(
                &floating,
                NumericKind::Float,
                NumericValue::Finite(FiniteNumber::float(1.0).unwrap()),
            ),
            NumericOperand::new(&equal, NumericKind::Decimal, decimal("1")),
            &context,
        );
        assert_eq!(
            actual.unwrap(),
            matches!(
                operator,
                ComparisonOperator::LessEqual | ComparisonOperator::GreaterEqual
            )
        );
        assert_eq!(
            context.flags.load(Ordering::Relaxed),
            1 << ComparisonSignal::FloatOperation as u8
        );
        let actual = compare_numeric(
            ComparisonRole::Range,
            operator,
            NumericOperand::new(
                &floating,
                NumericKind::Float,
                NumericValue::Finite(FiniteNumber::float(1.0).unwrap()),
            ),
            NumericOperand::new(
                &lower,
                NumericKind::Decimal,
                NumericValue::Finite(FiniteNumber::decimal("9", -1).unwrap()),
            ),
            &context,
        );
        assert!(!actual.unwrap());
    }
}
#[test]
fn property_pair_ordering_keeps_the_actual_pre314_literal_reflection_contract() {
    let left = literal("1.0", "double");
    let right = literal("1", "decimal");
    for operator in [ComparisonOperator::Less, ComparisonOperator::LessEqual] {
        let context = Context {
            float_trap: true,
            ..Context::default()
        };
        assert_eq!(
            compare_numeric(
                ComparisonRole::PropertyPair,
                operator,
                NumericOperand::new(
                    &left,
                    NumericKind::Float,
                    NumericValue::Finite(FiniteNumber::float(1.0).unwrap())
                ),
                NumericOperand::new(&right, NumericKind::Decimal, decimal("1")),
                &context
            ),
            if operator == ComparisonOperator::Less {
                Err(ComparisonFailure::FloatOperation)
            } else {
                Ok(true)
            }
        );
        assert_eq!(
            context.flags.load(Ordering::Relaxed),
            1 << ComparisonSignal::FloatOperation as u8
        );
    }
}
#[test]
fn finite_binary64_ordering_does_not_round_through_decimal_display() {
    let left = FiniteNumber::float(0.1).unwrap();
    let right = FiniteNumber::decimal("1", -1).unwrap();
    assert!(left > right);
    assert_eq!(
        FiniteNumber::float(-0.0).unwrap(),
        FiniteNumber::decimal("0", -1000).unwrap()
    );
    assert!(
        FiniteNumber::float(f64::from_bits(1)).unwrap() > FiniteNumber::decimal("0", 0).unwrap()
    );
    assert!(FiniteNumber::float(f64::INFINITY).is_err());
    assert!(FiniteNumber::float(f64::NAN).is_err());
}
#[test]
fn quiet_and_signaling_nan_keep_the_range_equality_then_order_signal_order() {
    let left = literal("NaN", "decimal");
    let right = literal("1", "double");
    let context = Context {
        float_trap: true,
        invalid_trap: true,
        ..Context::default()
    };
    assert!(
        !compare_numeric(
            ComparisonRole::Range,
            ComparisonOperator::GreaterEqual,
            NumericOperand::new(&left, NumericKind::Decimal, NumericValue::QuietNaN),
            NumericOperand::new(&right, NumericKind::Float, decimal("1")),
            &context
        )
        .unwrap()
    );
    assert_eq!(
        context.flags.load(Ordering::Relaxed),
        1 << ComparisonSignal::FloatOperation as u8
    );
    let context = Context {
        float_trap: true,
        invalid_trap: true,
        ..Context::default()
    };
    assert_eq!(
        compare_numeric(
            ComparisonRole::Range,
            ComparisonOperator::GreaterEqual,
            NumericOperand::new(&left, NumericKind::Decimal, NumericValue::SignalingNaN),
            NumericOperand::new(&right, NumericKind::Float, decimal("1")),
            &context
        ),
        Err(ComparisonFailure::InvalidOperation)
    );
    assert_eq!(context.flags.load(Ordering::Relaxed), 3);
}
#[test]
fn missing_values_are_not_numeric_comparison_errors_that_retry_sparql() {
    let left = literal("bad", "decimal");
    let right = literal("1", "decimal");
    let context = Context::default();
    assert!(
        !compare_numeric(
            ComparisonRole::Range,
            ComparisonOperator::GreaterEqual,
            NumericOperand::new(&left, NumericKind::Decimal, NumericValue::Missing),
            NumericOperand::new(&right, NumericKind::Decimal, decimal("1")),
            &context
        )
        .unwrap()
    );
    assert_eq!(context.flags.load(Ordering::Relaxed), 0);
}

#[test]
fn not_implemented_truth_uses_the_host_version_and_warning_disposition() {
    let left = literal("1.0", "double");
    let right = literal("1", "decimal");
    for (truth_error, warning_error, expected) in [
        (true, false, ComparisonFailure::NotImplementedTruth),
        (false, true, ComparisonFailure::DeprecationWarning),
    ] {
        let context = Context {
            float_trap: true,
            truth_error,
            warning_error,
            ..Context::default()
        };
        assert_eq!(
            compare_numeric(
                ComparisonRole::PropertyPair,
                ComparisonOperator::LessEqual,
                NumericOperand::new(
                    &left,
                    NumericKind::Float,
                    NumericValue::Finite(FiniteNumber::float(1.0).unwrap())
                ),
                NumericOperand::new(&right, NumericKind::Decimal, decimal("1")),
                &context
            ),
            Err(expected)
        );
        assert_eq!(
            context.flags.load(Ordering::Relaxed),
            1 << ComparisonSignal::FloatOperation as u8
        );
        assert_eq!(
            context.warnings.load(Ordering::Relaxed),
            u8::from(!truth_error)
        );
    }
}
