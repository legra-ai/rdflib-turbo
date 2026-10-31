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
    sort_focus_nodes(&mut nodes);
    assert_eq!(nodes, expected);
}

#[test]
fn literal_and_identifier_subsets_keep_the_previous_display_order() {
    let terms = [
        Term::from(NamedNode::new_unchecked("urn:a")),
        Term::from(NamedNode::new_unchecked("urn:a0")),
        Term::from(BlankNode::new_unchecked("a")),
        Term::from(Literal::new_simple_literal("line\n")),
        Term::from(Literal::new_simple_literal("line\t")),
        Literal::new_language_tagged_literal("é", "en")
            .unwrap()
            .into(),
        Literal::new_typed_literal("é", iri(XSD, "integer")).into(),
    ];
    for mask in 0..(1 << terms.len()) {
        let mut nodes: Vec<_> = terms
            .iter()
            .enumerate()
            .filter(|(index, _)| mask & (1 << index) != 0)
            .map(|(_, term)| term.clone())
            .collect();
        let mut expected = nodes.clone();
        expected.sort_by_key(ToString::to_string);
        sort_focus_nodes(&mut nodes);
        assert_eq!(nodes, expected);
    }
}
