use oxigraph::model::{BlankNode, GraphName, Literal, NamedNode, Quad};
use oxigraph::store::{LexicalTerm, Store};

use super::{BATCH_ROWS, TermView, open, start};

fn uri(value: &str) -> TermView<'_> {
    TermView::new("uri", value, None, None)
}

type ObservedTerm = (u8, String, Option<String>, Option<String>);
type ObservedRow = (
    Option<ObservedTerm>,
    Option<ObservedTerm>,
    Option<ObservedTerm>,
);

fn output_uri(value: &str) -> ObservedTerm {
    (0, value.into(), None, None)
}

fn observed_term(term: &Option<LexicalTerm>) -> Option<ObservedTerm> {
    term.as_ref().map(|term| match term {
        LexicalTerm::NamedNode(value) => output_uri(value.as_str()),
        LexicalTerm::BlankNode(value) => (1, value.as_str().into(), None, None),
        LexicalTerm::Literal {
            value,
            datatype,
            language,
        } => (
            2,
            value.as_str().into(),
            datatype.as_ref().map(|text| text.as_str().into()),
            language.as_ref().map(|text| text.as_str().into()),
        ),
    })
}

fn observed_rows(rows: &[super::Row]) -> Vec<ObservedRow> {
    // bounded: every tested native batch is capped at 128 rows.
    rows.iter()
        .map(|(s, p, o)| (observed_term(s), observed_term(p), observed_term(o)))
        .collect()
}

fn observed_batch((rows, finished): (Vec<super::Row>, bool)) -> (Vec<ObservedRow>, bool) {
    (observed_rows(&rows), finished)
}

fn insert(store: &Store, subject: &str, object: Literal, graph: GraphName) {
    store
        .insert(&Quad::new(
            NamedNode::new_unchecked(subject),
            NamedNode::new_unchecked("urn:p"),
            object,
            graph,
        ))
        .unwrap();
}

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
fn invalid_term_positions_and_masks_fail() {
    let store = Store::new().unwrap();
    let literal = || TermView::new("literal", "x", None, None);
    assert!(open(&store, Some(literal()), None, None, None, 7).is_err());
    assert!(open(&store, None, Some(literal()), None, None, 7).is_err());
    assert!(open(&store, None, None, None, Some(literal()), 7).is_err());
    assert!(open(&store, None, None, None, None, 8).is_err());
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

#[test]
fn zero_mask_checks_membership_without_returning_terms() {
    let store = Store::new().unwrap();
    insert(
        &store,
        "urn:s",
        Literal::new_simple_literal("v"),
        GraphName::DefaultGraph,
    );
    let cursor = open(
        &store,
        Some(uri("urn:s")),
        Some(uri("urn:p")),
        Some(TermView::new("literal", "v", None, None)),
        None,
        0,
    )
    .unwrap();
    assert_eq!(
        observed_batch(cursor.batch().unwrap()),
        (vec![(None, None, None)], true)
    );
}

#[test]
fn byte_budget_preserves_rows_across_pending_batches() {
    let store = Store::new().unwrap();
    for index in 0..100 {
        insert(
            &store,
            &format!("urn:s{index}"),
            Literal::new_simple_literal("x".repeat(1024)),
            GraphName::DefaultGraph,
        );
    }
    let cursor = open(&store, None, None, None, None, 4).unwrap();
    let (first, finished) = cursor.batch().unwrap();
    assert!(!finished);
    assert!(first.len() < 100);
    assert!(first.iter().map(super::row_bytes).sum::<usize>() <= super::BATCH_BYTES);
    let (second, finished) = cursor.batch().unwrap();
    assert_eq!(first.len() + second.len(), 100);
    assert!(finished);
    assert_eq!(observed_batch(cursor.batch().unwrap()), (vec![], true));
}

#[test]
fn initial_batch_finishes_small_and_empty_scoped_scans_without_a_cursor() {
    let store = Store::new().unwrap();
    insert(
        &store,
        "urn:s",
        Literal::new_language_tagged_literal("value", "en").unwrap(),
        NamedNode::new_unchecked("urn:g").into(),
    );
    let (rows, cursor) = start(&store, None, None, None, None, 7).unwrap();
    assert!(rows.is_empty());
    assert!(cursor.is_none());
    let (rows, cursor) = start(&store, None, None, None, Some(uri("urn:g")), 7).unwrap();
    assert_eq!(
        observed_rows(&rows),
        vec![(
            Some(output_uri("urn:s")),
            Some(output_uri("urn:p")),
            Some((2, "value".into(), None, Some("en".into())))
        )]
    );
    assert!(cursor.is_none());
}

#[test]
fn initial_batch_returns_a_cursor_only_when_the_scan_has_not_finished() {
    let store = Store::new().unwrap();
    for index in 0..(BATCH_ROWS + 3) {
        insert(
            &store,
            &format!("urn:s{index}"),
            Literal::new_simple_literal("v"),
            GraphName::DefaultGraph,
        );
    }
    let (first, cursor) = start(&store, None, None, None, None, 1).unwrap();
    assert_eq!(first.len(), BATCH_ROWS);
    let (last, finished) = cursor.unwrap().batch().unwrap();
    assert_eq!(last.len(), 3);
    assert!(finished);
}

#[test]
fn initial_byte_budget_retains_pending_rows_in_the_continuation() {
    let store = Store::new().unwrap();
    for index in 0..100 {
        insert(
            &store,
            &format!("urn:s{index}"),
            Literal::new_simple_literal("x".repeat(1024)),
            GraphName::DefaultGraph,
        );
    }
    let (first, cursor) = start(&store, None, None, None, None, 4).unwrap();
    assert!(first.len() < 100);
    assert!(first.iter().map(super::row_bytes).sum::<usize>() <= super::BATCH_BYTES);
    let (last, finished) = cursor.unwrap().batch().unwrap();
    assert_eq!(first.len() + last.len(), 100);
    assert!(finished);
}

#[test]
fn initial_zero_mask_returns_membership_without_a_cursor_or_terms() {
    let store = Store::new().unwrap();
    insert(
        &store,
        "urn:s",
        Literal::new_simple_literal("v"),
        GraphName::DefaultGraph,
    );
    let (rows, cursor) = start(&store, None, None, None, None, 0).unwrap();
    assert_eq!(observed_rows(&rows), vec![(None, None, None)]);
    assert!(cursor.is_none());
}

#[cfg(feature = "allocation-profiling")]
#[test]
#[ignore = "global allocation counters require this test to run alone with --test-threads=1"]
fn exact_membership_allocates_only_one_result_row() {
    let store = Store::new().unwrap();
    insert(
        &store,
        "urn:s",
        Literal::new_simple_literal("v"),
        GraphName::DefaultGraph,
    );
    let ((rows, cursor), bytes) = crate::allocation_tests::measure_allocated(|| {
        start(
            &store,
            Some(uri("urn:s")),
            Some(uri("urn:p")),
            Some(TermView::new("literal", "v", None, None)),
            None,
            0,
        )
        .unwrap()
    });
    assert_eq!(observed_rows(&rows), vec![(None, None, None)]);
    assert!(cursor.is_none());
    assert!(
        bytes <= std::mem::size_of::<super::Row>(),
        "Exact membership allocated a scan/batch instead of one row: {bytes} bytes"
    );
}

#[test]
fn exact_membership_preserves_graph_scope_lexicals_and_validation() {
    let store = Store::new().unwrap();
    let datatype = "http://www.w3.org/2001/XMLSchema#integer";
    store
        .insert(
            Quad::new(
                BlankNode::new_unchecked("subject"),
                NamedNode::new_unchecked("relative"),
                Literal::new_typed_literal("01", NamedNode::new_unchecked(datatype)),
                BlankNode::new_unchecked("graph"),
            )
            .as_ref(),
        )
        .unwrap();
    let subject = TermView::new("bnode", "subject", None, None);
    let predicate = uri("relative");
    let object = TermView::new("literal", "01", Some(datatype), None);
    let graph = TermView::new("bnode", "graph", None, None);
    let lookup = |object, graph| {
        let (rows, cursor) = start(
            &store,
            Some(subject),
            Some(predicate),
            Some(object),
            graph,
            0,
        )
        .unwrap();
        assert!(cursor.is_none());
        observed_rows(&rows)
    };
    assert_eq!(lookup(object, Some(graph)), vec![(None, None, None)]);
    assert!(lookup(object, None).is_empty());
    assert!(
        lookup(
            TermView::new("literal", "1", Some(datatype), None),
            Some(graph)
        )
        .is_empty()
    );
    assert!(lookup(object, Some(uri("graph"))).is_empty());
    for (subject, predicate, object, graph) in [
        (object, predicate, object, Some(graph)),
        (subject, subject, object, Some(graph)),
        (
            subject,
            predicate,
            TermView::new("literal", "x", None, Some("!")),
            Some(graph),
        ),
        (subject, predicate, object, Some(object)),
    ] {
        assert!(
            start(
                &store,
                Some(subject),
                Some(predicate),
                Some(object),
                graph,
                0
            )
            .is_err()
        );
    }
}

#[path = "read_lease_tests.rs"]
mod leases;
