//! Projection, scoping, batching and snapshot ownership of native reads.
use oxigraph::model::{GraphName, Literal, NamedNode};
use oxigraph::store::Store;

use super::super::{BATCH_ROWS, TermView, start};

use super::fixtures::*;

#[test]
fn projects_only_requested_terms_and_preserves_literal_identity() {
    let store = Store::new().unwrap();
    insert(
        &store,
        "relative",
        Literal::new_typed_literal(
            "01",
            NamedNode::new_unchecked("http://www.w3.org/2001/XMLSchema#integer"),
        ),
        GraphName::DefaultGraph,
    );
    let cursor = open(&store, Some(uri("relative")), None, None, None, 4).unwrap();
    let (rows, finished) = cursor.batch().unwrap();
    assert!(finished);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].0.is_none());
    assert!(rows[0].1.is_none());
    assert_eq!(
        observed_term(&rows[0].2),
        Some((
            2,
            "01".into(),
            Some("http://www.w3.org/2001/XMLSchema#integer".into()),
            None
        ))
    );
    assert_eq!(observed_batch(cursor.batch().unwrap()), (vec![], true));
}

#[test]
fn graph_none_means_default_and_named_graph_is_scoped() {
    let store = Store::new().unwrap();
    insert(
        &store,
        "urn:default",
        Literal::new_simple_literal("default"),
        GraphName::DefaultGraph,
    );
    insert(
        &store,
        "urn:named",
        Literal::new_language_tagged_literal("named", "en").unwrap(),
        NamedNode::new_unchecked("urn:g").into(),
    );
    let default = open(&store, None, None, None, None, 1).unwrap();
    assert_eq!(
        observed_batch(default.batch().unwrap()),
        (vec![(Some(output_uri("urn:default")), None, None)], true)
    );
    let named = open(&store, None, None, None, Some(uri("urn:g")), 4).unwrap();
    assert_eq!(
        observed_batch(named.batch().unwrap()),
        (
            vec![(
                None,
                None,
                Some((2, "named".into(), None, Some("en".into())))
            )],
            true
        )
    );
}

#[test]
fn batches_are_capped_and_close_releases_iterator() {
    let store = Store::new().unwrap();
    for index in 0..(BATCH_ROWS + 3) {
        insert(
            &store,
            &format!("urn:s{index}"),
            Literal::new_simple_literal("v"),
            GraphName::DefaultGraph,
        );
    }
    let cursor = open(&store, None, None, None, None, 7).unwrap();
    let (first, finished) = cursor.batch().unwrap();
    assert_eq!(first.len(), BATCH_ROWS);
    assert!(!finished);
    let (last, finished) = cursor.batch().unwrap();
    assert_eq!(last.len(), 3);
    assert!(finished);
    assert_eq!(observed_batch(cursor.batch().unwrap()), (vec![], true));
    let cursor = open(&store, None, None, None, None, 7).unwrap();
    cursor.release().unwrap();
    assert_eq!(observed_batch(cursor.batch().unwrap()), (vec![], true));
}

#[test]
fn oversized_rows_make_progress_without_unbounded_batching() {
    let store = Store::new().unwrap();
    for index in 0..3 {
        insert(
            &store,
            &format!("urn:s{index}"),
            Literal::new_simple_literal("x".repeat(70_000)),
            GraphName::DefaultGraph,
        );
    }
    let cursor = open(&store, None, None, None, None, 4).unwrap();
    for _ in 0..3 {
        let (rows, finished) = cursor.batch().unwrap();
        assert_eq!(rows.len(), 1);
        assert!(!finished);
    }
    assert_eq!(observed_batch(cursor.batch().unwrap()), (vec![], true));
}

#[test]
fn snapshots_are_owned_and_independent() {
    let store = Store::new().unwrap();
    insert(
        &store,
        "urn:before",
        Literal::new_simple_literal("v"),
        GraphName::DefaultGraph,
    );
    let before = open(&store, None, None, None, None, 1).unwrap();
    insert(
        &store,
        "urn:after",
        Literal::new_simple_literal("v"),
        GraphName::DefaultGraph,
    );
    let after = open(&store, None, None, None, None, 1).unwrap();
    drop(store);
    let (rows, finished) = before.batch().unwrap();
    assert_eq!(rows.len(), 1);
    assert!(finished);
    let (rows, finished) = after.batch().unwrap();
    assert_eq!(rows.len(), 2);
    assert!(finished);
}

#[test]
fn impossible_read_positions_are_empty_but_invalid_metadata_and_masks_fail() {
    let store = Store::new().unwrap();
    let literal = || TermView::new("literal", "x", None, None);
    assert!(
        start(&store, Some(literal()), None, None, None, 7)
            .unwrap()
            .0
            .is_empty()
    );
    assert!(
        start(&store, None, Some(literal()), None, None, 7)
            .unwrap()
            .0
            .is_empty()
    );
    assert!(open(&store, None, None, None, Some(literal()), 7).is_err());
    assert!(start(&store, None, None, None, None, 8).is_err());
    assert!(
        open(
            &store,
            None,
            None,
            Some(TermView::new("unknown", "x", None, None)),
            None,
            7
        )
        .is_err()
    );
}
