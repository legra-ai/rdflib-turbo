//! Consuming a full-Term frontier releases vacated structural backing storage.
use super::Pending;
use oxigraph::model::{GeneralizedQuad, GraphName, Literal, NamedNode};

#[cfg(feature = "allocation-profiling")]
#[test]
#[ignore = "global allocation counters require isolated execution"]
fn consumed_generalized_frontier_releases_storage_before_iteration_finishes() {
    const FACTS: usize = 4_096;
    // bounded: exactly FACTS short full-Term heads in this allocation regression.
    let mut pending = Pending::default();
    for index in 0..FACTS {
        pending.insert(GeneralizedQuad::new(
            Literal::new_simple_literal(format!("subject-{index}")),
            NamedNode::new_unchecked("urn:p"),
            Literal::new_simple_literal("value"),
            GraphName::DefaultGraph,
        ));
    }
    let mut output = pending.into_iter();
    let before = crate::allocation_tests::live_requested_bytes();
    for _ in 0..FACTS / 2 {
        drop(output.next().unwrap());
    }
    let reclaimed = before.saturating_sub(crate::allocation_tests::live_requested_bytes());
    println!("FRONTIER_RETIREMENT reclaimed_bytes={reclaimed}");
    assert!(
        reclaimed >= FACTS * std::mem::size_of::<GeneralizedQuad>() / 4,
        "consumed generalized frontier backing remained allocated: reclaimed {reclaimed} bytes"
    );
    assert_eq!(output.count(), FACTS / 2);
}

#[test]
fn generalized_frontier_preserves_all_term_roles_and_context_identity() {
    use oxigraph::model::{BlankNode, Term, vocab::xsd};
    use std::collections::HashSet;
    // bounded: nine exact fixture terms, including lexical and explicit-string distinctions.
    let terms: [Term; 9] = [
        NamedNode::new_unchecked("urn:value").into(),
        BlankNode::new_unchecked("value").into(),
        Literal::new_simple_literal("value").into(),
        Literal::new_typed_literal("value", xsd::STRING).into(),
        Literal::new_language_tagged_literal("value", "en")
            .unwrap()
            .into(),
        Literal::new_language_tagged_literal("value", "fr")
            .unwrap()
            .into(),
        Literal::new_typed_literal("01", xsd::INTEGER).into(),
        Literal::new_typed_literal("1", xsd::INTEGER).into(),
        Literal::new_simple_literal("urn:value").into(),
    ];
    let graphs = [
        GraphName::DefaultGraph,
        NamedNode::new_unchecked("urn:graph").into(),
        BlankNode::new_unchecked("graph").into(),
    ];
    // bounded: exactly 9^3*3 full-Term/context combinations, never caller data.
    let mut expected = HashSet::new();
    let mut pending = Pending::default();
    for subject in &terms {
        for predicate in &terms {
            for object in &terms {
                for graph in &graphs {
                    let row = GeneralizedQuad::new(
                        subject.clone(),
                        predicate.clone(),
                        object.clone(),
                        graph.clone(),
                    );
                    expected.insert(row.clone());
                    pending.insert(row);
                }
            }
        }
    }
    // bounded: the same finite 2,187-row fixture's equality/comparator matrix.
    for left in &expected {
        for right in &expected {
            let order = Pending::compare_quads(left, right);
            assert_eq!(order == std::cmp::Ordering::Equal, left == right);
            assert_eq!(order, Pending::compare_quads(right, left).reverse());
        }
    }
    let actual: HashSet<_> = pending.into_iter().collect();
    assert_eq!(actual, expected);
}
