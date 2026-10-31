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
        nodes.sort_by(focus_node_order);
        assert_eq!(nodes, expected);
    }
}

#[test]
fn literal_formatter_retains_the_original_complete_unicode_snapshot() {
    use sha2::{Digest, Sha256};
    // bounded: every Unicode scalar, independent of caller data size.
    let value: String = (0..=0x10_FFFF).filter_map(char::from_u32).collect();
    let text = Literal::new_simple_literal(value).to_string();
    let digest: String = Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    println!("ORIGINAL_LITERAL_UNICODE_SHA256 {digest}");
    assert_eq!(
        digest,
        "7d442e99ad6d1dac47c34ff2a3195f3dbfb6ec9db1580e4091d11fc96e661709"
    );
}
