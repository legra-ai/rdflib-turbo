//! Query rows retain the actual source dictionary rather than cloning words.
use oxigraph::model::*;
use spareval::QueryableDataset;

use crate::QueryGraph;

fn node(word: &str) -> NamedNode {
    NamedNode::new(format!("urn:borrowed-query:{word}")).unwrap()
}

#[test]
fn dataset_query_rows_borrow_the_source_literal_word() {
    let value = Literal::new_simple_literal("source-word".repeat(8192));
    let mut source = Dataset::new();
    source.insert(&Quad::new(
        node("s"),
        node("p"),
        value,
        GraphName::DefaultGraph,
    ));
    let expected = source.iter().next().unwrap().object;
    let TermRef::Literal(expected) = expected else {
        panic!("literal fixture")
    };
    let query = QueryGraph::new(&source);
    let row = query
        .internal_quads_for_pattern(None, None, None, Some(None))
        .next()
        .unwrap()
        .unwrap();
    let TermRef::Literal(actual) = TermRef::from(&row.object) else {
        panic!("literal row")
    };
    assert_eq!(actual.value(), expected.value());
    assert_eq!(
        actual.value().as_ptr(),
        expected.value().as_ptr(),
        "query cloned the source word"
    );
}

#[test]
fn generalized_query_rows_borrow_all_generalized_source_words() {
    let mut source = GeneralizedDataset::new();
    source.insert(&GeneralizedQuad::new(
        Literal::new_simple_literal("subject-word".repeat(8192)),
        Literal::new_simple_literal("predicate-word".repeat(8192)),
        Literal::new_simple_literal("object-word".repeat(8192)),
        GraphName::DefaultGraph,
    ));
    let expected = source.iter().next().unwrap();
    let query = QueryGraph::new(&source);
    let row = query
        .internal_quads_for_pattern(None, None, None, Some(None))
        .next()
        .unwrap()
        .unwrap();
    for (actual, expected) in [
        (TermRef::from(&row.subject), expected.subject),
        (TermRef::from(&row.predicate), expected.predicate),
        (TermRef::from(&row.object), expected.object),
    ] {
        let (TermRef::Literal(actual), TermRef::Literal(expected)) = (actual, expected) else {
            panic!("generalized literals")
        };
        assert_eq!(actual.value(), expected.value());
        assert_eq!(
            actual.value().as_ptr(),
            expected.value().as_ptr(),
            "query cloned a generalized word"
        );
    }
}
