use std::sync::{Arc, Mutex};

use oxigraph::model::{GraphName, Literal, NamedNode, Quad};
use oxigraph::store::Store;

use super::{QuadInput, TermTuple};
use crate::store::operations::Operation;
use crate::store::session::{Control, Session};

fn uri(value: &str) -> TermTuple {
    ("uri".into(), value.into(), None, None)
}

fn insertion(subject: TermTuple, object: TermTuple, graph: Option<TermTuple>) -> Operation {
    Operation::InsertTerms(Box::new(QuadInput::new(
        subject,
        uri("urn:p"),
        object,
        graph,
    )))
}

#[test]
fn typed_insert_preserves_relative_iris_blank_nodes_and_exact_literals() {
    let store = Store::new().unwrap();
    let control = Arc::new(Mutex::new(Control::new(store.clone())));
    let object = (
        "literal".into(),
        "01".into(),
        Some("http://www.w3.org/2001/XMLSchema#integer".into()),
        None,
    );
    Session::execute_inline(control.clone(), insertion(uri("relative"), object, None)).unwrap();
    assert!(
        store
            .contains(&Quad::new(
                NamedNode::new_unchecked("relative"),
                NamedNode::new_unchecked("urn:p"),
                Literal::new_typed_literal(
                    "01",
                    NamedNode::new_unchecked("http://www.w3.org/2001/XMLSchema#integer")
                ),
                GraphName::DefaultGraph
            ))
            .unwrap()
    );
    Session::execute_inline(
        control,
        insertion(
            ("bnode".into(), "subject".into(), None, None),
            ("literal".into(), "hello".into(), None, Some("en".into())),
            Some(uri("urn:g")),
        ),
    )
    .unwrap();
    assert_eq!(store.len().unwrap(), 2);
}

#[test]
fn invalid_typed_operation_aborts_prior_transaction_writes() {
    let store = Store::new().unwrap();
    let control = Arc::new(Mutex::new(Control::new(store.clone())));
    let session = Session::start(control.clone()).unwrap();
    session
        .execute(insertion(uri("urn:s"), uri("urn:o"), None))
        .unwrap();
    let bad_subject = ("literal".into(), "bad".into(), None, None);
    assert!(
        session
            .execute(insertion(bad_subject, uri("urn:o"), None))
            .is_err()
    );
    assert_eq!(store.len().unwrap(), 0);
    assert!(session.finish(true).is_err());
    Session::execute_inline(control, insertion(uri("urn:new"), uri("urn:o"), None)).unwrap();
    assert_eq!(store.len().unwrap(), 1);
}

#[test]
fn shared_validation_rejects_invalid_positions_and_metadata() {
    let literal = || ("literal".into(), "x".into(), None, None);
    assert!(
        QuadInput::new(uri("urn:s"), literal(), uri("urn:o"), None)
            .quad()
            .is_err()
    );
    assert!(
        QuadInput::new(uri("urn:s"), uri("urn:p"), uri("urn:o"), Some(literal()))
            .quad()
            .is_err()
    );
    assert!(super::term(("uri".into(), "urn:x".into(), Some("urn:type".into()), None)).is_err());
    assert!(
        super::term((
            "literal".into(),
            "x".into(),
            Some("urn:type".into()),
            Some("en".into())
        ))
        .is_err()
    );
    assert!(
        super::term((
            "literal".into(),
            "x".into(),
            None,
            Some("not a language".into())
        ))
        .is_err()
    );
}

#[test]
fn blank_node_graph_identity_survives_typed_transaction_commit() {
    let store = Store::new().unwrap();
    let control = Arc::new(Mutex::new(Control::new(store.clone())));
    let session = Session::start(control).unwrap();
    session
        .execute(insertion(
            uri("urn:s"),
            uri("urn:o"),
            Some(("bnode".into(), "named-context".into(), None, None)),
        ))
        .unwrap();
    assert_eq!(store.len().unwrap(), 0);
    session.finish(true).unwrap();
    let quad = store.iter().next().unwrap().unwrap();
    assert_eq!(
        quad.graph_name,
        oxigraph::model::BlankNode::new_unchecked("named-context").into()
    );
}

#[test]
fn identifiers_reject_literal_metadata_in_every_transport_position() {
    // bounded: two identifier kinds and three invalid metadata combinations.
    for kind in ["uri", "bnode"] {
        for (datatype, language) in [
            (Some("urn:datatype"), None),
            (None, Some("en")),
            (Some("urn:datatype"), Some("en")),
        ] {
            let invalid = (
                kind.into(),
                "identifier".into(),
                datatype.map(Into::into),
                language.map(Into::into),
            );
            assert!(super::term(invalid.clone()).is_err());
            assert!(
                QuadInput::new(invalid.clone(), uri("urn:p"), uri("urn:o"), None)
                    .quad()
                    .is_err()
            );
            assert!(
                QuadInput::new(uri("urn:s"), invalid.clone(), uri("urn:o"), None)
                    .quad()
                    .is_err()
            );
            assert!(
                QuadInput::new(uri("urn:s"), uri("urn:p"), invalid.clone(), None)
                    .quad()
                    .is_err()
            );
            assert!(
                QuadInput::new(uri("urn:s"), uri("urn:p"), uri("urn:o"), Some(invalid))
                    .quad()
                    .is_err()
            );
        }
    }
}
