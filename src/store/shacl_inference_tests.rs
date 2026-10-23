//! Validation sees inference without publishing base or derived private state.
use std::sync::{Arc, Mutex};

use oxigraph::io::RdfFormat;
use oxigraph::model::*;
use oxigraph::store::Store;
use pyshacl_turbo_engine::{CompiledShapes, Options, parse_graph};

use super::*;
use crate::inference::InferenceRequest;
use crate::shacl::InferenceMode as Mode;
use crate::store::operations::{Operation, Reply};
use crate::store::session::{Control, Session};

fn node(local: &str) -> NamedNode {
    NamedNode::new(format!("urn:{local}")).unwrap()
}

fn shapes() -> Arc<CompiledShapes> {
    let graph = parse_graph(
        "@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:S> sh:targetSubjectsOf <urn:parent>; sh:class <urn:Required>.",
        RdfFormat::Turtle, None,
    ).unwrap();
    Arc::new(CompiledShapes::compile(graph, false).unwrap())
}

fn request(mode: Mode) -> InferenceStage {
    InferenceStage {
        mode,
        request: InferenceRequest::default(),
    }
}

fn seed(transaction: &mut oxigraph::store::Transaction<'_>, graph: GraphName) {
    transaction.insert(&Quad::new(
        node("s"),
        node("child"),
        node("o"),
        graph.clone(),
    ));
    transaction.insert(&Quad::new(
        node("child"),
        NamedNode::new_unchecked("http://www.w3.org/2000/01/rdf-schema#subPropertyOf"),
        node("parent"),
        graph,
    ));
}

#[test]
fn inference_validation_private_delta_preserves_named_blank_and_default_scope() {
    for mode in [Mode::Rdfs, Mode::Owlrl, Mode::Both] {
        for graph in [
            GraphName::DefaultGraph,
            node("named").into(),
            BlankNode::new("selected").unwrap().into(),
        ] {
            let store = Store::new().unwrap();
            let mut transaction = store.start_transaction().unwrap();
            seed(&mut transaction, graph.clone());
            // Tempting membership outside the selected graph must stay invisible.
            transaction.insert(&Quad::new(
                node("s"),
                NamedNode::new_unchecked("http://www.w3.org/1999/02/22-rdf-syntax-ns#type"),
                node("Required"),
                node("outside"),
            ));
            let (report, delta) = validate(
                &transaction,
                &shapes(),
                Options::default(),
                true,
                Some(graph.clone()),
                request(mode),
            )
            .unwrap();
            assert!(!report.into_output().0);
            assert_eq!(transaction.len().unwrap(), 3);
            assert!(!delta.inference.is_empty());
            assert!(
                delta
                    .inference
                    .iter()
                    .all(|quad| quad.graph_name.is_default_graph())
            );
            delta.apply(&mut transaction);
            assert!(
                transaction
                    .quads_for_pattern(
                        Some(node("s").as_ref().into()),
                        Some(node("parent").as_ref()),
                        Some(node("o").as_ref().into()),
                        Some(graph.as_ref())
                    )
                    .next()
                    .unwrap()
                    .is_ok()
            );
        }
    }
}

#[test]
fn readonly_inference_uses_committed_snapshot_without_reserving_active_writer() {
    let store = Store::new().unwrap();
    let mut transaction = store.start_transaction().unwrap();
    seed(&mut transaction, GraphName::DefaultGraph);
    transaction.commit().unwrap();
    let control = Arc::new(Mutex::new(Control::new(store.clone())));
    let writer = Session::start(control.clone()).unwrap();
    let source = crate::store::NativeStore { control };
    let report = source
        .validate_inferred_readonly(&shapes(), Options::default(), None, request(Mode::Rdfs))
        .unwrap();
    assert!(!report.into_output().0);
    assert_eq!(store.len().unwrap(), 2);
    writer.finish(false).unwrap();
}

#[test]
fn pending_transaction_inference_accepts_only_derived_output_and_cas_failure_rolls_back() {
    for changed in [false, true] {
        let store = Store::new().unwrap();
        let control = Arc::new(Mutex::new(Control::new(store.clone())));
        let session = Session::start(control).unwrap();
        session.execute(Operation::Update("INSERT DATA {<urn:s> <urn:child> <urn:o>. <urn:child> <http://www.w3.org/2000/01/rdf-schema#subPropertyOf> <urn:parent>}".into(), None)).unwrap();
        let Reply::PreparedValidation(output) = session
            .execute(Operation::ValidateInferred {
                shapes: shapes(),
                options: Options::default(),
                inplace: true,
                graph: None,
                inference: Box::new(request(Mode::Rdfs)),
            })
            .unwrap()
        else {
            panic!("prepared validation")
        };
        let (report, revision) = *output;
        assert!(!report.into_output().0);
        assert!(matches!(
            session.execute(Operation::Len).unwrap(),
            Reply::Count(2)
        ));
        assert_eq!(store.len().unwrap(), 0);
        if changed {
            session
                .execute(Operation::Update(
                    "INSERT DATA {<urn:mutation> <urn:p> <urn:o>}".into(),
                    None,
                ))
                .unwrap();
            assert!(session.accept_validation(revision).is_err());
            assert_eq!(store.len().unwrap(), 0);
        } else {
            session.accept_validation(revision).unwrap();
            session.finish(true).unwrap();
            assert!(
                store
                    .quads_for_pattern(
                        Some(node("s").as_ref().into()),
                        Some(node("parent").as_ref()),
                        Some(node("o").as_ref().into()),
                        Some(node("pyshacl:inference").as_ref().into())
                    )
                    .next()
                    .unwrap()
                    .is_ok()
            );
        }
    }
}
