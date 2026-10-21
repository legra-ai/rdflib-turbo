use super::*;
use oxigraph::model::NamedNode;
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    datatype: String,
    lexical: String,
    expected: String,
}

#[test]
fn pinned_actual_altxsd_converter_acceptance() {
    let context = LexicalContext::default();
    let mut mismatches = 0;
    for (index, line) in include_str!("reference.jsonl").lines().enumerate() {
        let fixture: Fixture = serde_json::from_str(line).unwrap();
        let literal =
            Literal::new_typed_literal(fixture.lexical, NamedNode::new(fixture.datatype).unwrap());
        let expected = match fixture.expected.as_str() {
            "valid" => LexicalResult::Valid,
            "diagnostic" => LexicalResult::Diagnostic,
            "InvalidOperation" => LexicalResult::Failure(LexicalFailure::DecimalInvalidOperation),
            "IndexError" => LexicalResult::Failure(LexicalFailure::IndexError),
            name => panic!("unexpected reference failure {name}"),
        };
        let actual = check(&literal, &context);
        if actual != expected {
            eprintln!(
                "fixture {}: {literal}: got {actual:?}, expected {expected:?}",
                index + 1
            );
            mismatches += 1;
        }
    }
    assert_eq!(mismatches, 0);
}

fn typed(value: &str, kind: &str) -> Literal {
    Literal::new_typed_literal(
        value,
        NamedNode::new_unchecked(format!("http://www.w3.org/2001/XMLSchema#{kind}")),
    )
}

#[test]
fn lexical_context_is_owned_send_and_scalar_lookups_work_detached() {
    pyo3::Python::initialize();
    pyo3::Python::attach(|py| {
        let context = LexicalContext::capture(py).unwrap();
        py.detach(move || {
            std::thread::spawn(move || {
                assert_eq!(
                    check(&typed("١_٢", "integer"), &context),
                    LexicalResult::Valid
                );
                assert!(unicode::word('β'));
                assert!(unicode::word('²'));
                assert!(unicode::whitespace('\u{1c}'));
                assert!(!unicode::word('!'));
            })
            .join()
            .unwrap()
        });
    });
}

#[test]
fn scalar_tables_match_running_interpreter() {
    use pyo3::prelude::*;
    pyo3::Python::initialize();
    pyo3::Python::attach(|py| {
        let module = py.import("unicodedata").unwrap();
        // bounded: fixed regression alphabet, including newer Nd and non-Nd numeric characters.
        for ch in [
            '0',
            '١',
            '９',
            '\u{1e4f0}',
            '²',
            '½',
            'β',
            '中',
            '_',
            '\u{1c}',
            '\u{a0}',
            '!',
        ] {
            let value = ch.to_string();
            let string = pyo3::types::PyString::new(py, &value);
            let expected: Option<u8> = module
                .call_method1("decimal", (&value, py.None()))
                .unwrap()
                .extract()
                .unwrap();
            assert_eq!(unicode::decimal(ch), expected, "decimal {ch}");
            let word: bool = string.call_method0("isalnum").unwrap().extract().unwrap();
            assert_eq!(unicode::word(ch), word || ch == '_', "word {ch}");
            let whitespace: bool = string.call_method0("isspace").unwrap().extract().unwrap();
            assert_eq!(unicode::whitespace(ch), whitespace, "space {ch}");
        }
    });
}

#[test]
fn identity_converters_do_not_apply_a_different_namespace_profile() {
    let context = LexicalContext::default();
    for datatype in [
        "urn:datatype:integer",
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#integer",
        "http://www.w3.org/2001/XMLSchema#XMLLiteral",
    ] {
        let literal = Literal::new_typed_literal("bad lexical", NamedNode::new_unchecked(datatype));
        assert_eq!(check(&literal, &context), LexicalResult::Valid);
    }
}

#[test]
fn fractional_datetime_conversion_obeys_integer_digit_limit() {
    let value = format!("2026-01-01T01:02:03.{}1", "0".repeat(4300));
    let literal = typed(&value, "dateTime");
    let mut context = LexicalContext::default();
    assert_eq!(check(&literal, &context), LexicalResult::Diagnostic);
    context.integer_digit_limit = 0;
    assert_eq!(check(&literal, &context), LexicalResult::Valid);
}

#[derive(Deserialize)]
struct ContextFixture {
    precision: usize,
    rounding: String,
    emin: i64,
    emax: i64,
    clamp: u8,
    traps: [bool; 7],
    kind: String,
    lexical: String,
    expected: String,
}

#[test]
fn decimal_conversion_uses_the_callers_rounding_bounds_and_traps() {
    use super::context::{DecimalContext, Rounding};
    // bounded: three fixed reference fixture sets.
    for file in [
        include_str!("float-contexts.jsonl"),
        include_str!("double-contexts.jsonl"),
        include_str!("decimal-contexts.jsonl"),
    ] {
        for (index, line) in file.lines().enumerate() {
            let fixture: ContextFixture = serde_json::from_str(line).unwrap();
            let rounding = match fixture.rounding.as_str() {
                "ROUND_CEILING" => Rounding::Ceiling,
                "ROUND_DOWN" => Rounding::Down,
                "ROUND_FLOOR" => Rounding::Floor,
                "ROUND_HALF_DOWN" => Rounding::HalfDown,
                "ROUND_HALF_EVEN" => Rounding::HalfEven,
                "ROUND_HALF_UP" => Rounding::HalfUp,
                "ROUND_UP" => Rounding::Up,
                "ROUND_05UP" => Rounding::ZeroFiveUp,
                _ => panic!("unknown fixture rounding"),
            };
            let context = LexicalContext {
                decimal: DecimalContext {
                    precision: fixture.precision,
                    rounding,
                    emin: fixture.emin,
                    emax: fixture.emax,
                    clamp: fixture.clamp != 0,
                    traps: fixture.traps,
                },
                ..Default::default()
            };
            let expected = match fixture.expected.as_str() {
                "valid" => LexicalResult::Valid,
                "diagnostic" => LexicalResult::Diagnostic,
                "InvalidOperation" => {
                    LexicalResult::Failure(LexicalFailure::DecimalInvalidOperation)
                }
                other => panic!("unexpected reference failure {other}"),
            };
            assert_eq!(
                check(&typed(&fixture.lexical, &fixture.kind), &context),
                expected,
                "{} context fixture {}: {} {} precision {}",
                fixture.kind,
                index + 1,
                fixture.lexical,
                fixture.rounding,
                fixture.precision
            );
        }
    }
}

#[test]
fn context_capture_reuses_the_existing_interpreter_time_pattern() {
    use pyo3::prelude::*;
    pyo3::Python::initialize();
    pyo3::Python::attach(|py| {
        let module = py.import("_strptime").unwrap();
        let original = module.getattr("TimeRE").unwrap();
        module.setattr("TimeRE", py.None()).unwrap();
        let captured = LexicalContext::capture(py);
        module.setattr("TimeRE", original).unwrap();
        assert!(
            captured.is_ok(),
            "capture must read the existing immutable hour pattern"
        );
    });
}

#[derive(Deserialize)]
struct CardinalityFixture {
    datatype: String,
    lexical: String,
    invalid_trap: bool,
    expected: String,
}

#[test]
fn cardinality_zero_uses_rdflib_values_separately_from_alt_diagnostics() {
    for line in include_str!("cardinality-zero.jsonl").lines() {
        let fixture: CardinalityFixture = serde_json::from_str(line).unwrap();
        let mut context = LexicalContext::default();
        context.decimal.traps[context::INVALID] = fixture.invalid_trap;
        let literal =
            Literal::new_typed_literal(fixture.lexical, NamedNode::new_unchecked(fixture.datatype));
        let expected = if fixture.expected == "InvalidComparison" {
            Err(LexicalFailure::DecimalInvalidComparison)
        } else {
            Ok(fixture.expected == "true")
        };
        assert_eq!(cardinality_zero(&literal, &context), expected, "{literal}");
    }
}
