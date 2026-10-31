//! The optimized comparator retains the previous formatted-key ordering exactly.
use super::*;

#[test]
fn identifier_prefixes_delimiters_and_mixed_terms_keep_display_order() {
    let mut nodes = Vec::new();
    for value in [
        "", "urn:a", "urn:a0", "urn:a/", "urn:a>", "urn:a>>", "urn:é", "urn:😀",
    ] {
        nodes.push(Term::from(NamedNode::new_unchecked(value)));
        nodes.push(Term::from(BlankNode::new_unchecked(value)));
    }
    for value in ["", "text", "quoted\"", "line\n", "tab\t", "é", "😀"] {
        nodes.push(Term::from(Literal::new_simple_literal(value)));
        nodes.push(
            Literal::new_language_tagged_literal(value, "en")
                .unwrap()
                .into(),
        );
        nodes.push(Literal::new_typed_literal(value, iri(XSD, "string")).into());
    }
    for left in &nodes {
        for right in &nodes {
            assert_eq!(
                focus_node_order(left, right),
                left.to_string().cmp(&right.to_string()),
                "{left} versus {right}"
            );
        }
    }
    let mut expected = nodes.clone();
    expected.sort_by_key(ToString::to_string);
    nodes.sort_by(focus_node_order);
    assert_eq!(nodes, expected);
}
