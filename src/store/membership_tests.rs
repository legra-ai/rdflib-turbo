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
