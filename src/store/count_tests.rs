use oxigraph::model::{GraphName, Literal, NamedNode, Quad};
use oxigraph::store::Store;

use super::{Pattern, count};

#[test]
fn unfiltered_graph_count_never_constructs_a_scan() {
    let pattern = Pattern::parse(r#"[null,null,null,""]"#).unwrap();
    assert_eq!(
        pattern
            .count(
                |graph| {
                    assert_eq!(graph, GraphName::DefaultGraph.as_ref());
                    Ok(123)
                },
                || panic!("Unfiltered graph cardinality opened a scan"),
            )
            .unwrap(),
        123
    );
}

#[test]
fn union_and_selective_counts_still_scan_their_actual_matches() {
    let store = fixture();
    for (source, expected) in [
        (r#"[null,null,null,null]"#, 1000),
        (r#"[{"type":"uri","value":"urn:s0"},null,null,""]"#, 1),
    ] {
        let pattern = Pattern::parse(source).unwrap();
        let actual = pattern
            .count(
                |_| panic!("Selective or union count used whole-graph cardinality"),
                || {
                    store.generalized_quads_for_pattern(
                        pattern.subject.as_ref().map(|term| term.as_ref()),
                        pattern.predicate.as_ref().map(|term| term.as_ref()),
                        pattern.object.as_ref().map(|term| term.as_ref()),
                        pattern.graph.as_ref().map(|term| term.as_ref()),
                    )
                },
            )
            .unwrap();
        assert_eq!(actual, expected);
    }
}

fn fixture() -> Store {
    let store = Store::new().unwrap();
    let predicate = NamedNode::new_unchecked("urn:p");
    let object = Literal::new_simple_literal("x".repeat(4096));
    for index in 0..1000 {
        let subject = NamedNode::new_unchecked(format!("urn:s{index}"));
        for graph in [
            GraphName::DefaultGraph,
            NamedNode::new_unchecked("urn:g").into(),
        ] {
            store
                .insert(&Quad::new(
                    subject.clone(),
                    predicate.clone(),
                    object.clone(),
                    graph,
                ))
                .unwrap();
        }
    }
    store
}

#[test]
fn scoped_and_union_counts_preserve_cardinality_through_transaction_changes() {
    let store = fixture();
    assert_eq!(
        count(
            store.generalized_quads_for_pattern(None, None, None, None),
            true
        )
        .unwrap(),
        1000
    );
    assert_eq!(
        count(
            store.generalized_quads_for_pattern(
                None,
                None,
                None,
                Some(GraphName::DefaultGraph.as_ref())
            ),
            false
        )
        .unwrap(),
        1000
    );
    let mut transaction = store.start_transaction().unwrap();
    transaction.insert(&Quad::new(
        NamedNode::new_unchecked("urn:new"),
        NamedNode::new_unchecked("urn:p"),
        Literal::new_simple_literal("new"),
        GraphName::DefaultGraph,
    ));
    assert_eq!(
        count(
            transaction.generalized_quads_for_pattern(
                None,
                None,
                None,
                Some(GraphName::DefaultGraph.as_ref())
            ),
            false
        )
        .unwrap(),
        1001
    );
    assert_eq!(
        count(
            store.generalized_quads_for_pattern(None, None, None, None),
            true
        )
        .unwrap(),
        1000
    );
    transaction.commit().unwrap();
    assert_eq!(
        count(
            store.generalized_quads_for_pattern(None, None, None, None),
            true
        )
        .unwrap(),
        1001
    );
}

#[cfg(feature = "allocation-profiling")]
#[test]
#[ignore = "global allocation counters require this test to run alone with --test-threads=1"]
fn scoped_count_does_not_allocate_decoded_lexical_payloads() {
    let store = fixture();
    let graph = GraphName::DefaultGraph;
    let (result, bytes) = crate::allocation_tests::measure_allocated(|| {
        count(
            store.generalized_quads_for_pattern(None, None, None, Some(graph.as_ref())),
            false,
        )
        .unwrap()
    });
    assert_eq!(result, 1000);
    assert!(
        bytes < 64 * 1024,
        "Scoped count decoded unused lexical payloads: {bytes} bytes requested"
    );
}

#[cfg(feature = "allocation-profiling")]
fn long_union_fixture() -> Store {
    use oxigraph::model::{BlankNode, GeneralizedQuad};
    let store = Store::new().unwrap();
    let graphs = [
        GraphName::DefaultGraph,
        NamedNode::new_unchecked(format!("urn:scope:{}", "g".repeat(8192))).into(),
        BlankNode::new_unchecked(format!("scope{}", "g".repeat(8192))).into(),
    ];
    for index in 0..128 {
        for graph in &graphs {
            store
                .insert_generalized(&GeneralizedQuad::new(
                    Literal::new_simple_literal(format!("subject{index}:{}", "s".repeat(8192))),
                    BlankNode::new_unchecked(format!("predicate{}", "p".repeat(8192))),
                    Literal::new_typed_literal(
                        format!("object{index}:{}", "o".repeat(8192)),
                        NamedNode::new_unchecked(format!("urn:datatype:{}", "d".repeat(8192))),
                    ),
                    graph.clone(),
                ))
                .unwrap();
        }
    }
    store
}

#[cfg(feature = "allocation-profiling")]
#[test]
#[ignore = "global allocation counters require this test to run alone with --test-threads=1"]
fn union_count_does_not_allocate_decoded_lexical_payloads() {
    let store = long_union_fixture();
    let (result, bytes) = crate::allocation_tests::measure_allocated(|| {
        count(
            store.generalized_quads_for_pattern(None, None, None, None),
            true,
        )
        .unwrap()
    });
    assert_eq!(result, 128);
    println!("UNION_COUNT requested_bytes={bytes}");
    assert!(
        bytes < 16 * 1024,
        "Union count copied lexical payloads or retained a statement-sized set: {bytes} requested bytes"
    );
}

#[cfg(feature = "allocation-profiling")]
#[test]
#[ignore = "global allocation counters require this test to run alone with --test-threads=1"]
fn transaction_union_count_does_not_allocate_decoded_lexical_payloads() {
    use oxigraph::model::{GeneralizedQuad, GraphNameRef};
    let store = long_union_fixture();
    let mut transaction = store.start_transaction().unwrap();
    transaction.insert_generalized(&GeneralizedQuad::new(
        NamedNode::new_unchecked("urn:pending"),
        NamedNode::new_unchecked("urn:p"),
        Literal::new_simple_literal("pending"),
        GraphNameRef::DefaultGraph,
    ));
    let (result, bytes) = crate::allocation_tests::measure_allocated(|| {
        count(
            transaction.generalized_quads_for_pattern(None, None, None, None),
            true,
        )
        .unwrap()
    });
    assert_eq!(result, 129);
    println!("TRANSACTION_UNION_COUNT requested_bytes={bytes}");
    assert!(
        bytes < 16 * 1024,
        "Pending union count copied lexical payloads or retained a statement-sized set: {bytes} requested bytes"
    );
}

#[test]
fn encoded_union_counts_preserve_all_masks_full_term_roles_and_graph_kinds() {
    use oxigraph::model::{BlankNode, GeneralizedQuad, Term};
    use std::collections::HashSet;
    let store = Store::new().unwrap();
    let subjects: [Term; 3] = [
        NamedNode::new_unchecked("urn:role").into(),
        BlankNode::new_unchecked("urn:role").into(),
        Literal::new_simple_literal("urn:role").into(),
    ];
    let predicates: [Term; 3] = [
        NamedNode::new_unchecked("urn:predicate").into(),
        BlankNode::new_unchecked("urn:predicate").into(),
        Literal::new_simple_literal("urn:predicate").into(),
    ];
    let objects: [Term; 2] = [
        Literal::new_simple_literal("value").into(),
        Literal::new_typed_literal(
            "value",
            NamedNode::new_unchecked("http://www.w3.org/2001/XMLSchema#string"),
        )
        .into(),
    ];
    let graphs = [
        GraphName::DefaultGraph,
        NamedNode::new_unchecked("urn:scope").into(),
        BlankNode::new_unchecked("urn:scope").into(),
    ];
    let mut rows = Vec::new();
    for s in &subjects {
        for p in &predicates {
            for o in &objects {
                for g in &graphs {
                    let row = GeneralizedQuad::new(s.clone(), p.clone(), o.clone(), g.clone());
                    store.insert_generalized(&row).unwrap();
                    rows.push(row);
                }
            }
        }
    }
    let base = rows.clone();
    let mut transaction = store.start_transaction().unwrap();
    transaction.remove_generalized(&rows[0]);
    rows.remove(0);
    let pending = GeneralizedQuad::new(
        NamedNode::new_unchecked("urn:pending"),
        predicates[0].clone(),
        objects[0].clone(),
        graphs[2].clone(),
    );
    transaction.insert_generalized(&pending);
    rows.push(pending);
    let absent = Term::from(NamedNode::new_unchecked("urn:absent"));
    for (pending, expected_rows) in [(false, &base), (true, &rows)] {
        for subject in [&subjects[2], &absent] {
            for mask in 0..8 {
                for graph in [None, Some(&graphs[0]), Some(&graphs[1]), Some(&graphs[2])] {
                    let s = (mask & 1 != 0).then_some(subject);
                    let p = (mask & 2 != 0).then_some(&predicates[1]);
                    let o = (mask & 4 != 0).then_some(&objects[1]);
                    let expected: HashSet<_> = expected_rows
                        .iter()
                        .filter(|q| {
                            s.is_none_or(|v| q.subject == *v)
                                && p.is_none_or(|v| q.predicate == *v)
                                && o.is_none_or(|v| q.object == *v)
                                && graph.is_none_or(|v| q.graph_name == *v)
                        })
                        .map(|q| (q.subject.clone(), q.predicate.clone(), q.object.clone()))
                        .collect();
                    let iter = if pending {
                        transaction.generalized_quads_for_pattern(
                            s.map(|v| v.as_ref()),
                            p.map(|v| v.as_ref()),
                            o.map(|v| v.as_ref()),
                            graph.map(|v| v.as_ref()),
                        )
                    } else {
                        store.generalized_quads_for_pattern(
                            s.map(|v| v.as_ref()),
                            p.map(|v| v.as_ref()),
                            o.map(|v| v.as_ref()),
                            graph.map(|v| v.as_ref()),
                        )
                    };
                    assert_eq!(
                        count(iter, graph.is_none()).unwrap(),
                        expected.len(),
                        "pending={pending},mask={mask},graph={graph:?}"
                    );
                }
            }
        }
    }
}
