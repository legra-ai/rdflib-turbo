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
                    store.quads_for_pattern(
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
    assert_eq!(count(store.iter(), true).unwrap(), 1000);
    assert_eq!(
        count(
            store.quads_for_pattern(None, None, None, Some(GraphName::DefaultGraph.as_ref())),
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
            transaction.quads_for_pattern(None, None, None, Some(GraphName::DefaultGraph.as_ref())),
            false
        )
        .unwrap(),
        1001
    );
    assert_eq!(count(store.iter(), true).unwrap(), 1000);
    transaction.commit().unwrap();
    assert_eq!(count(store.iter(), true).unwrap(), 1001);
}

#[cfg(feature = "allocation-profiling")]
#[test]
#[ignore = "global allocation counters require this test to run alone with --test-threads=1"]
fn scoped_count_does_not_allocate_decoded_lexical_payloads() {
    let store = fixture();
    let graph = GraphName::DefaultGraph;
    let (result, bytes) = crate::allocation_tests::measure_allocated(|| {
        count(
            store.quads_for_pattern(None, None, None, Some(graph.as_ref())),
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
