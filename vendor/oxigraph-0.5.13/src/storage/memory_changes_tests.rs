//! Changed-only inference capture keeps initial presence and an owning lease.
use super::*;
use crate::model::{BlankNode, GeneralizedQuad, Literal, NamedNode};
use crate::store::{LexicalGraphName, LexicalTerm, Store};
use std::collections::HashSet;

fn row(id: usize) -> GeneralizedQuad {
    GeneralizedQuad::new(
        Literal::new_simple_literal(format!("subject-{id}")),
        BlankNode::new(format!("predicate-{id}")).unwrap(),
        NamedNode::new(format!("urn:object:{id}")).unwrap(),
        GraphNameRef::DefaultGraph,
    )
}

#[test]
fn captured_changes_cancel_mutations_and_preserve_first_record_identity() {
    let storage = MemoryStorage::new();
    let originals = [row(0), row(1)];
    let mut transaction = storage.start_transaction();
    for quad in &originals {
        transaction.insert(quad);
    }
    transaction.commit();
    let original = storage
        .content
        .quad_set
        .get(&EncodedQuad::from(originals[1].as_ref()))
        .unwrap()
        .clone();
    let mut transaction = storage.start_transaction();
    transaction.begin_changes().unwrap();
    transaction.remove(&row(99)); // Missing removal must not enter the journal.
    transaction.insert(&row(2));
    transaction.remove(&row(2));
    transaction.insert(&row(2));
    transaction.insert(&row(3));
    transaction.remove(&row(3));
    transaction.remove(&originals[0]);
    transaction.insert(&originals[0]);
    transaction.remove(&originals[1]);
    let changes = transaction.commit_with_changes().unwrap();
    assert_eq!(changes.added.len(), 1);
    assert_eq!(changes.added[0].quad, EncodedQuad::from(row(2).as_ref()));
    assert_eq!(changes.removed.len(), 1);
    assert!(Arc::ptr_eq(&changes.removed[0], &original));
    assert!(changes.reader.contains(&EncodedQuad::from(row(2).as_ref())));
    assert!(!changes.reader.contains(&original.quad));
}

#[test]
fn captured_reader_pins_commit_before_later_writer_and_outlives_owner() {
    let storage = MemoryStorage::new();
    let quad = GeneralizedQuad::new(
        Literal::new_simple_literal("subject with a long dictionary value"),
        BlankNode::new("long-predicate-identity").unwrap(),
        Literal::new_typed_literal("0007", NamedNode::new("urn:datatype").unwrap()),
        BlankNode::new("scope").unwrap(),
    );
    let mut transaction = storage.start_transaction();
    transaction.begin_changes().unwrap();
    transaction.insert(&quad);
    let changes = transaction.commit_with_changes().unwrap();
    let mut later = storage.start_transaction();
    later.remove(&quad);
    later.commit();
    assert!(
        !storage
            .snapshot()
            .contains(&EncodedQuad::from(quad.as_ref()))
    );
    assert!(changes.reader.contains(&EncodedQuad::from(quad.as_ref())));
    drop(storage);
    assert_eq!(
        changes
            .reader
            .decode_generalized_quad(&changes.added[0].quad)
            .unwrap(),
        quad
    );
}

#[test]
fn capture_rejects_prior_writes_and_error_drop_restores_cleanup_state() {
    let storage = MemoryStorage::new();
    let mut transaction = storage.start_transaction();
    transaction.insert(&row(0));
    assert!(transaction.begin_changes().is_err());
    drop(transaction);
    assert!(storage.snapshot().is_empty());
    let mut initial = storage.start_transaction();
    initial.insert(&row(0));
    initial.commit();
    let mut transaction = storage.start_transaction();
    transaction.begin_changes().unwrap();
    assert!(transaction.begin_changes().is_err());
    transaction.insert(&row(1));
    transaction.remove(&row(0));
    drop(transaction); // A late inference or cleanup error aborts both directions.
    let reader = storage.snapshot();
    assert!(reader.contains(&EncodedQuad::from(row(0).as_ref())));
    assert!(!reader.contains(&EncodedQuad::from(row(1).as_ref())));
}

#[test]
fn captured_bulk_cleanup_keeps_exact_graph_kind_and_initial_final_delta() {
    let storage = MemoryStorage::new();
    let graphs = [
        GraphNameRef::DefaultGraph.into_owned(),
        NamedNode::new("urn:scope").unwrap().into(),
        BlankNode::new("urn:scope").unwrap().into(),
    ];
    let mut initial = storage.start_transaction();
    for graph in &graphs {
        let mut quad = row(0);
        quad.graph_name = graph.clone();
        initial.insert(&quad);
    }
    initial.commit();
    let mut transaction = storage.start_transaction();
    transaction.begin_changes().unwrap();
    transaction.clear_graph(graphs[1].as_ref());
    let changes = transaction.commit_with_changes().unwrap();
    assert!(changes.added.is_empty());
    assert_eq!(
        changes
            .removed
            .iter()
            .map(|node| node.quad.graph_name.clone())
            .collect::<HashSet<_>>(),
        HashSet::from([graphs[1].as_ref().into()])
    );
    assert_eq!(changes.reader.len(), 2);
}

#[test]
fn optional_capture_keeps_zero_off_state_and_only_pointer_sized_added_rows() {
    // bounded: 256 pre-existing MVCC identities isolate capture from storage writes.
    const ROWS: usize = 256;
    let storage = MemoryStorage::new();
    let mut initial = storage.start_transaction();
    for id in 0..ROWS {
        initial.insert(&row(id));
    }
    initial.commit();
    let nodes: Vec<_> = storage
        .content
        .quad_set
        .iter()
        .map(|node| Arc::clone(&node))
        .collect();
    let mut transaction = storage.start_transaction();
    assert!(transaction.capture.is_none());
    assert_eq!(size_of_val(&transaction.capture), size_of::<usize>());
    let off = lock_tests::measure_allocations(|| {
        for node in &nodes {
            transaction.capture_insert(node);
        }
    });
    assert_eq!(off, (0, 0));
    let (_, bytes) = lock_tests::measure_allocations(|| {
        transaction.begin_changes().unwrap();
        for node in &nodes {
            transaction.capture_insert(node);
        }
    });
    let Some(capture) = transaction.capture.as_deref() else {
        panic!("missing capture")
    };
    let Capture::Additive(rows) = capture else {
        panic!("unexpected promotion")
    };
    let pointer = size_of::<Arc<QuadListNode>>();
    assert_eq!(pointer, size_of::<usize>());
    assert_eq!(rows.capacity() * pointer, ROWS * pointer);
    assert!(
        bytes <= 2 * ROWS * pointer + 256,
        "capture copied more than Arc-vector growth: {bytes}"
    );
    assert!(
        rows.iter()
            .zip(&nodes)
            .all(|(captured, original)| Arc::ptr_eq(captured, original))
    );
}

#[test]
fn committed_full_quad_projection_leases_lexical_words_without_string_copies() {
    let store = Store::new().unwrap();
    let subject = "subject".repeat(1024);
    let predicate = "predicate".repeat(1024);
    let object = "object".repeat(1024);
    let graph = format!("urn:graph:{}", "g".repeat(4096));
    let quad = GeneralizedQuad::new(
        Literal::new_language_tagged_literal(subject.clone(), "en").unwrap(),
        Literal::new_simple_literal(predicate.clone()),
        Literal::new_typed_literal(object.clone(), NamedNode::new("urn:datatype").unwrap()),
        NamedNode::new(graph.clone()).unwrap(),
    );
    let mut transaction = store.start_transaction().unwrap();
    transaction.begin_memory_changes().unwrap();
    transaction.insert_generalized(&quad);
    let (added, removed) = transaction.commit_with_changes().unwrap().into_parts();
    let mut cursor = added.project_quad_lexical();
    let mut projected = None;
    let allocations = lock_tests::measure_allocations(|| {
        projected = Some(cursor.next().unwrap().unwrap());
    });
    assert_eq!(
        allocations,
        (0, 0),
        "full quad projection copied dictionary words"
    );
    drop((cursor, removed, store));
    let projected = projected.unwrap();
    let LexicalTerm::Literal {
        value, language, ..
    } = projected.subject
    else {
        panic!("subject kind")
    };
    assert_eq!(value.as_str(), subject);
    assert_eq!(language.unwrap().as_str(), "en");
    let LexicalTerm::Literal { value, .. } = projected.predicate else {
        panic!("predicate kind")
    };
    assert_eq!(value.as_str(), predicate);
    let LexicalTerm::Literal {
        value, datatype, ..
    } = projected.object
    else {
        panic!("object kind")
    };
    assert_eq!(value.as_str(), object);
    assert_eq!(datatype.unwrap().as_str(), "urn:datatype");
    let LexicalGraphName::NamedNode(value) = projected.graph_name else {
        panic!("graph kind")
    };
    assert_eq!(value.as_str(), graph);
}

#[test]
fn prepared_capture_keeps_caller_unpublished_and_requires_complete_acceptance() {
    let storage = MemoryStorage::new();
    let mut initial = storage.start_transaction();
    initial.insert(&row(0));
    initial.commit();
    let mut transaction = storage.start_transaction();
    transaction.begin_changes().unwrap();
    transaction.remove(&row(0));
    transaction.insert(&row(1));
    let mut prepared = transaction.prepare_changes().unwrap();
    assert!(
        storage
            .snapshot()
            .contains(&EncodedQuad::from(row(0).as_ref()))
    );
    assert!(
        !storage
            .snapshot()
            .contains(&EncodedQuad::from(row(1).as_ref()))
    );
    assert!(transaction.ensure_no_capture().is_err());
    assert!(transaction.prepare_changes().is_err());
    transaction.validate_prepared(&prepared).unwrap();
    assert!(!prepared.finished());
    assert_eq!(
        prepared.next(true),
        Some(EncodedQuad::from(row(1).as_ref()))
    );
    assert_eq!(
        prepared.next(false),
        Some(EncodedQuad::from(row(0).as_ref()))
    );
    assert!(prepared.finished());
    transaction.commit_prepared_changes(prepared).unwrap();
    assert!(
        !storage
            .snapshot()
            .contains(&EncodedQuad::from(row(0).as_ref()))
    );
    assert!(
        storage
            .snapshot()
            .contains(&EncodedQuad::from(row(1).as_ref()))
    );
    let mut transaction = storage.start_transaction();
    transaction.begin_changes().unwrap();
    transaction.remove(&row(1));
    transaction.insert(&row(2));
    let prepared = transaction.prepare_changes().unwrap();
    drop(prepared);
    drop(transaction);
    assert!(
        storage
            .snapshot()
            .contains(&EncodedQuad::from(row(1).as_ref()))
    );
    assert!(
        !storage
            .snapshot()
            .contains(&EncodedQuad::from(row(2).as_ref()))
    );
}

#[test]
fn prepared_removal_discard_consumes_ids_without_decoding_or_accepting_additions() {
    let storage = MemoryStorage::new();
    let mut initial = storage.start_transaction();
    initial.insert(&row(0));
    initial.commit();
    let mut transaction = storage.start_transaction();
    transaction.begin_changes().unwrap();
    transaction.remove(&row(0));
    transaction.insert(&row(1));
    let mut prepared = transaction.prepare_changes().unwrap();
    prepared.discard_removals();
    prepared.discard_removals();
    assert_eq!(prepared.next(false), None);
    assert!(!prepared.finished());
    assert!(
        storage
            .snapshot()
            .contains(&EncodedQuad::from(row(0).as_ref()))
    );
    assert_eq!(
        prepared.next(true),
        Some(EncodedQuad::from(row(1).as_ref()))
    );
    assert!(prepared.finished());
    transaction.commit_prepared_changes(prepared).unwrap();
    assert!(
        !storage
            .snapshot()
            .contains(&EncodedQuad::from(row(0).as_ref()))
    );
    assert!(
        storage
            .snapshot()
            .contains(&EncodedQuad::from(row(1).as_ref()))
    );
}
