use super::*;
use crate::boolean_input::live::{FocusOwner, Owner};
use pyo3::types::PyDict;
use pyshacl_turbo_engine::{BuiltinBooleanState, SelectedLiteralOwner, oxigraph::model::Literal};
use std::any::Any;

const BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";

fn batch() -> Batch {
    (
        vec![
            (1, "result".into(), None, None),
            (
                0,
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".into(),
                None,
                None,
            ),
            (
                0,
                "http://www.w3.org/ns/shacl#ValidationResult".into(),
                None,
                None,
            ),
            (0, "http://www.w3.org/ns/shacl#value".into(), None, None),
            (2, "false".into(), Some(BOOLEAN.into()), None),
        ],
        vec![(0, 1, 2, None), (0, 3, 4, None)],
    )
}

fn issuer(py: Python<'_>) -> Arc<Issuer> {
    Arc::new(Issuer {
        interpreter: crate::comparison::interpreter(py).unwrap(),
    })
}

fn literal(py: Python<'_>) -> Py<PyAny> {
    let scope = PyDict::new(py);
    py.run(
        c"from rdflib import Literal
from rdflib.namespace import XSD
class Observed(Literal):
 @property
 def value(self): raise AssertionError('delivery read value')
 @property
 def ill_typed(self): raise AssertionError('delivery read assessment')
 @property
 def datatype(self): raise AssertionError('delivery read datatype')
 @property
 def language(self): raise AssertionError('delivery read language')
value=Observed('false',datatype=XSD.boolean,normalize=False)",
        Some(&scope),
        None,
    )
    .unwrap();
    scope.get_item("value").unwrap().unwrap().unbind()
}

fn owner(py: Python<'_>, issuer: Arc<Issuer>, receipt: usize) -> SelectedLiteral {
    SelectedLiteral(Arc::new(Owner {
        literal: literal(py),
        issuer,
        word: false,
        receipt,
    }))
}

fn rejects(batch: Batch, entries: Vec<(usize, BooleanObservation)>, expected: &str) {
    let Err(Error::Invalid(message)) = SelectedReport::new(batch, entries) else {
        panic!("Invalid report receipt must fail before delivery");
    };
    assert_eq!(message, expected);
}

#[test]
fn delivery_preserves_actual_receiver_without_metadata_reads_and_is_single_use() {
    Python::initialize();
    Python::attach(|py| {
        let receiver = literal(py);
        let selected = SelectedLiteral(Arc::new(Owner {
            literal: receiver.clone_ref(py),
            issuer: issuer(py),
            word: false,
            receipt: 1,
        }));
        let expected = batch();
        let mut report = SelectedReport::new(
            expected.clone(),
            vec![(1, BooleanObservation::Reported(selected))],
        )
        .unwrap();
        let (actual, states, live) = report.take_parts(py).unwrap();
        assert_eq!(actual, expected);
        assert!(states.is_empty());
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].0, 1);
        assert!(live[0].1.bind(py).is(receiver.bind(py)));
        assert!(
            report
                .take_parts(py)
                .unwrap_err()
                .is_instance_of::<PyValueError>(py)
        );
    });
}

#[test]
fn constructor_rejects_duplicate_unissued_mixed_and_missing_issuers() {
    Python::initialize();
    Python::attach(|py| {
        let selected = owner(py, issuer(py), 1);
        rejects(
            batch(),
            vec![
                (1, BooleanObservation::Reported(selected.clone())),
                (1, BooleanObservation::Reported(selected.clone())),
            ],
            "Duplicate selected report occurrence",
        );
        rejects(
            batch(),
            vec![(1, BooleanObservation::Reported(owner(py, issuer(py), 0)))],
            "Unissued selected report receipt",
        );
        let mut twice = batch();
        twice.1.push((0, 3, 4, None));
        rejects(
            twice,
            vec![
                (1, BooleanObservation::Reported(selected)),
                (2, BooleanObservation::Reported(owner(py, issuer(py), 2))),
            ],
            "Selected report mixes caller issuers",
        );
        rejects(
            batch(),
            vec![(
                1,
                BooleanObservation::Attested(BuiltinBooleanState::FalseValid),
            )],
            "Selected report has no live issuer",
        );
    });
}

#[test]
fn constructor_checks_quad_term_role_word_and_result_context() {
    Python::initialize();
    Python::attach(|py| {
        for defect in [
            "quad",
            "term",
            "predicate",
            "kind",
            "word",
            "datatype",
            "language",
            "context",
            "result",
        ] {
            let mut source = batch();
            let mut index = 1;
            let reason = if defect == "quad" {
                index = 2;
                "Invalid selected report quad index"
            } else if defect == "term" {
                source.1[1].2 = 99;
                "Invalid selected report term index"
            } else {
                match defect {
                    "predicate" => source.1[1].1 = 1,
                    "kind" => source.0[4].0 = 0,
                    "word" => source.0[4].1 = "true".into(),
                    "datatype" => source.0[4].2 = None,
                    "language" => source.0[4].3 = Some("en".into()),
                    "context" => source.1[1].3 = Some(0),
                    "result" => {
                        source.1.remove(0);
                    }
                    _ => unreachable!(),
                };
                if defect == "result" {
                    index = 0;
                }
                "Selected receipt is not its admitted direct SH.value/SH.focusNode word"
            };
            rejects(
                source,
                vec![(
                    index,
                    BooleanObservation::Reported(owner(py, issuer(py), 1)),
                )],
                reason,
            );
        }
    });
}

#[derive(Debug)]
struct Foreign;
impl SelectedLiteralOwner for Foreign {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[test]
fn foreign_owner_type_cannot_impersonate_a_source_receipt() {
    rejects(
        batch(),
        vec![(
            1,
            BooleanObservation::Reported(SelectedLiteral(Arc::new(Foreign))),
        )],
        "Foreign selected report owner",
    );
}

#[test]
fn focus_and_attested_occurrences_keep_separate_delivery_authorities() {
    Python::initialize();
    Python::attach(|py| {
        let receiver = literal(py);
        let selected = SelectedLiteral(Arc::new(FocusOwner {
            literal: receiver.clone_ref(py),
            issuer: issuer(py),
            receipt: 1,
            word: Literal::new_typed_literal(
                "false",
                pyshacl_turbo_engine::oxigraph::model::NamedNode::new_unchecked(BOOLEAN),
            ),
        }));
        let mut source = batch();
        source.0[3].1 = "http://www.w3.org/ns/shacl#focusNode".into();
        source.0.push((1, "other-result".into(), None, None));
        source
            .0
            .push((0, "http://www.w3.org/ns/shacl#value".into(), None, None));
        source.1.extend([(5, 1, 2, None), (5, 6, 4, None)]);
        let mut report = SelectedReport::new(
            source.clone(),
            vec![
                (1, BooleanObservation::Reported(selected)),
                (
                    3,
                    BooleanObservation::Attested(BuiltinBooleanState::TrueInvalid),
                ),
            ],
        )
        .unwrap();
        let (actual, states, live) = report.take_parts(py).unwrap();
        assert_eq!(actual, source);
        assert_eq!(states, vec![(3, 6)]);
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].0, 1);
        assert!(live[0].1.bind(py).is(receiver.bind(py)));
    });
}

#[test]
fn wrong_interpreter_delivery_does_not_consume_the_payload() {
    Python::initialize();
    Python::attach(|py| {
        let foreign = Arc::new(Issuer {
            interpreter: crate::comparison::interpreter(py)
                .unwrap()
                .checked_add(1)
                .unwrap(),
        });
        let mut report = SelectedReport::new(
            batch(),
            vec![(1, BooleanObservation::Reported(owner(py, foreign, 1)))],
        )
        .unwrap();
        let error = report.take_parts(py).unwrap_err();
        assert!(error.is_instance_of::<PyValueError>(py));
        assert!(error.to_string().contains("another interpreter"));
        assert!(report.payload.is_some());
    });
}

#[test]
fn clear_releases_delivery_authority() {
    Python::initialize();
    Python::attach(|py| {
        let mut report = SelectedReport::new(
            batch(),
            vec![(1, BooleanObservation::Reported(owner(py, issuer(py), 1)))],
        )
        .unwrap();
        report.__clear__();
        assert!(
            report
                .take_parts(py)
                .unwrap_err()
                .is_instance_of::<PyValueError>(py)
        );
    });
}

#[test]
fn reported_literal_cycles_are_visible_to_python_gc_without_getter_calls() {
    Python::initialize();
    Python::attach(|py| {
        let receiver = literal(py);
        let selected = SelectedLiteral(Arc::new(Owner {
            literal: receiver.clone_ref(py),
            issuer: issuer(py),
            word: false,
            receipt: 1,
        }));
        let report = Py::new(
            py,
            SelectedReport::new(batch(), vec![(1, BooleanObservation::Reported(selected))])
                .unwrap(),
        )
        .unwrap();
        receiver
            .bind(py)
            .setattr("report", report.bind(py))
            .unwrap();
        let weak = py
            .import("weakref")
            .unwrap()
            .getattr("ref")
            .unwrap()
            .call1((receiver.bind(py),))
            .unwrap();
        drop(receiver);
        drop(report);
        py.import("gc").unwrap().call_method0("collect").unwrap();
        assert!(weak.call0().unwrap().is_none());
    });
}
