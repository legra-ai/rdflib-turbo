use oxigraph::model::{BlankNode, GeneralizedQuad, GraphName, Literal, NamedNode, Quad};
use oxigraph::store::Store;
use std::sync::{Arc, Mutex};

use super::super::operations::{Operation, Reply};
use super::super::session::{Control, Session};

#[test]
fn explicit_copy_preserves_generalized_input_roles() {
    let store = Store::new().unwrap();
    let mut transaction = store.start_transaction().unwrap();
    let row = GeneralizedQuad::new(
        Literal::new_simple_literal("literal subject"),
        BlankNode::new("predicate").unwrap(),
        NamedNode::new("urn:value").unwrap(),
        GraphName::DefaultGraph,
    );
    transaction.insert_generalized(&row);
    let copied = super::fork(&transaction, None).unwrap();
    let rows = copied
        .generalized_quads_for_pattern(None, None, None, None)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(rows, vec![row]);
    assert_eq!(copied.len().unwrap(), 1);
}

#[test]
fn committed_copy_does_not_reserve_a_second_writer_or_read_pending_rows() {
    pyo3::Python::initialize();
    pyo3::Python::attach(|py| {
        let store = Store::new().unwrap();
        let existing = Quad::new(
            NamedNode::new("urn:existing").unwrap(),
            NamedNode::new("urn:p").unwrap(),
            Literal::new_simple_literal("committed"),
            GraphName::DefaultGraph,
        );
        store.insert(&existing).unwrap();
        let owner = super::NativeStore::from_store(store);
        let writer = Session::start(owner.control.clone()).unwrap();
        writer
            .execute(Operation::Load {
                source: "<urn:pending> <urn:p> 'pending'.".into(),
                format: oxigraph::io::RdfFormat::Turtle,
                base: None,
            })
            .unwrap();
        let copied = owner.fork_graph(py, None).unwrap();
        let copied = copied.snapshot().unwrap();
        assert_eq!(copied.len().unwrap(), 1);
        assert!(copied.contains(&existing).unwrap());
        writer.finish(false).unwrap();
    });
}

#[test]
fn copied_native_owner_preserves_empty_contexts_exact_terms_and_pending_rows() {
    let original = Store::new().unwrap();
    let mut source = original.start_transaction().unwrap();
    let graph = BlankNode::new("graph").unwrap();
    let empty = NamedNode::new("urn:empty").unwrap();
    let quad = Quad::new(
        BlankNode::new("subject").unwrap(),
        NamedNode::new("urn:p").unwrap(),
        Literal::new_typed_literal("01", NamedNode::new("urn:datatype").unwrap()),
        graph.clone(),
    );
    source.insert(&quad);
    source.insert_named_graph(&empty);
    let copied = super::fork(&source, None).unwrap();
    drop(source);
    assert!(original.is_empty().unwrap());
    assert!(copied.contains(&quad).unwrap());
    assert!(copied.contains_named_graph(&empty).unwrap());
    drop(original);
    assert!(copied.contains_named_graph(&graph).unwrap());
    assert_eq!(copied.len().unwrap(), 1);
}

#[test]
fn scoped_copy_excludes_unrelated_contexts_and_keeps_an_empty_selected_graph() {
    let original = Store::new().unwrap();
    let mut source = original.start_transaction().unwrap();
    let empty = NamedNode::new("urn:empty").unwrap();
    source.insert_named_graph(&empty);
    source.insert(&Quad::new(
        NamedNode::new("urn:s").unwrap(),
        NamedNode::new("urn:p").unwrap(),
        Literal::new_simple_literal("outside"),
        GraphName::DefaultGraph,
    ));
    let selected = super::fork(
        &source,
        Some("{\"type\":\"uri\",\"value\":\"urn:empty\"}".into()),
    )
    .unwrap();
    assert!(selected.is_empty().unwrap());
    assert!(selected.contains_named_graph(&empty).unwrap());
    assert_eq!(selected.named_graphs().count(), 1);
}

#[test]
fn failed_copy_on_transaction_worker_aborts_pending_source_writes() {
    let store = Store::new().unwrap();
    let control = Arc::new(Mutex::new(Control::new(store.clone())));
    let session = Session::start(control.clone()).unwrap();
    session
        .execute(Operation::Update(
            "INSERT DATA { <urn:s> <urn:p> <urn:o> }".into(),
            None,
        ))
        .unwrap();
    assert!(
        session
            .execute(Operation::ForkGraph(Some("not-json".into())))
            .is_err()
    );
    assert!(session.finish(true).is_err());
    assert!(store.is_empty().unwrap());
    assert!(matches!(
        Session::execute_inline(control, Operation::ForkGraph(None)).unwrap(),
        Reply::Store(_)
    ));
}

#[test]
fn copying_pending_quads_keeps_prepared_validation_acceptable() {
    let store = Store::new().unwrap();
    let control = Arc::new(Mutex::new(Control::new(store.clone())));
    let session = Session::start(control).unwrap();
    session
        .execute(Operation::Update(
            "INSERT DATA { <urn:s> <urn:p> <urn:o> }".into(),
            None,
        ))
        .unwrap();
    let shapes = pyshacl_turbo_engine::CompiledShapes::compile(
        pyshacl_turbo_engine::IndexedShapesSource::parse_graph(
            "",
            oxigraph::io::RdfFormat::Turtle,
            None,
        )
        .unwrap(),
        false,
    )
    .unwrap();
    let Reply::PreparedValidation(report) = session
        .execute(Operation::Validate {
            shapes: Arc::new(shapes),
            options: Default::default(),
            inplace: true,
            graph: None,
        })
        .unwrap()
    else {
        panic!("Expected prepared report")
    };
    let (_, revision) = *report;
    let Reply::Store(copied) = session.execute(Operation::ForkGraph(None)).unwrap() else {
        panic!("Expected independent copied store")
    };
    assert_eq!(copied.len().unwrap(), 1);
    assert!(store.is_empty().unwrap());
    session.accept_validation(revision).unwrap();
    session.finish(true).unwrap();
    assert_eq!(store.len().unwrap(), 1);
    assert_eq!(copied.len().unwrap(), 1);
}
