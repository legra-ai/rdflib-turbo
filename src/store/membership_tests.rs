use oxigraph::model::{GraphName, Literal, NamedNode, Quad};
use oxigraph::store::Store;

use super::{TermView, contains_matching};

fn uri(value: &str) -> TermView<'_> {
    TermView::new("uri", value, None, None)
}

#[test]
fn boolean_membership_matches_every_binding_mask_in_its_graph() {
    let store = Store::new().unwrap();
    store
        .insert(&Quad::new(
            NamedNode::new_unchecked("urn:s"),
            NamedNode::new_unchecked("urn:p"),
            Literal::new_simple_literal("value"),
            GraphName::DefaultGraph,
        ))
        .unwrap();
    store
        .insert(&Quad::new(
            NamedNode::new_unchecked("urn:named-s"),
            NamedNode::new_unchecked("urn:p"),
            Literal::new_simple_literal("named"),
            NamedNode::new_unchecked("urn:g"),
        ))
        .unwrap();
    for mask in 0..8 {
        let subject = (mask & 1 != 0).then(|| uri("urn:s"));
        let predicate = (mask & 2 != 0).then(|| uri("urn:p"));
        let object = (mask & 4 != 0).then(|| TermView::new("literal", "value", None, None));
        assert!(contains_matching(&store, subject, predicate, object, None).unwrap());
        let named_match =
            contains_matching(&store, subject, predicate, object, Some(uri("urn:g"))).unwrap();
        assert_eq!(named_match, mask & 5 == 0);
        assert!(
            !contains_matching(&store, subject, predicate, object, Some(uri("urn:missing")),)
                .unwrap()
        );
    }
}

#[test]
fn boolean_membership_of_empty_default_and_declared_graphs_is_false() {
    let store = Store::new().unwrap();
    store
        .insert_named_graph(&NamedNode::new_unchecked("urn:empty"))
        .unwrap();
    assert!(!contains_matching(&store, None, None, None, None).unwrap());
    assert!(!contains_matching(&store, None, None, None, Some(uri("urn:empty"))).unwrap());
}

#[cfg(feature = "allocation-profiling")]
#[test]
#[ignore = "global allocation counters require this test to run alone with --test-threads=1"]
fn wildcard_boolean_membership_does_not_decode_stored_literal_text() {
    let store = Store::new().unwrap();
    // bounded: one large field; fixture setup precedes allocation accounting.
    store
        .insert(&Quad::new(
            NamedNode::new_unchecked("urn:s"),
            NamedNode::new_unchecked("urn:p"),
            Literal::new_simple_literal("x".repeat(4 * 1024 * 1024)),
            GraphName::DefaultGraph,
        ))
        .unwrap();
    let (found, bytes) = crate::allocation_tests::measure_allocated(|| {
        contains_matching(&store, Some(uri("urn:s")), Some(uri("urn:p")), None, None).unwrap()
    });
    assert!(found);
    assert!(
        bytes < 64 * 1024,
        "Boolean membership decoded unused stored text: {bytes} Rust heap bytes requested"
    );
}
