//! Query rows retain the actual source dictionary rather than cloning words.
use crate::{GraphRead, OverlayGraph, ScopedGraph, UnionGraph};
use oxigraph::model::*;
use oxigraph::store::Store;
use spareval::{InternalQuad, QueryableDataset, ScopedCustomFunction, TermCow};
use std::collections::HashMap;

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

fn owned(row: InternalQuad<TermCow<'_>>) -> GeneralizedQuad {
    GeneralizedQuad::new(
        Term::from(row.subject),
        Term::from(row.predicate),
        Term::from(row.object),
        match row.graph_name.map(Term::from) {
            Some(Term::NamedNode(node)) => node.into(),
            Some(Term::BlankNode(node)) => node.into(),
            None => GraphName::DefaultGraph,
            _ => panic!("invalid graph kind"),
        },
    )
}

fn check_masks(source: &impl GraphRead) {
    // bounded: each fixture/view below exposes at most twelve physical rows.
    let targets: Vec<_> = source
        .matching_quads(None, None, None, None)
        .map(Result::unwrap)
        .collect();
    for target in targets {
        for mask in 0..16 {
            let s = (mask & 1 != 0).then_some(target.subject.as_ref());
            let p = (mask & 2 != 0).then_some(target.predicate.as_ref());
            let o = (mask & 4 != 0).then_some(target.object.as_ref());
            let g = (mask & 8 != 0).then_some(target.graph_name.as_ref());
            let expected = source
                .matching_quads(s, p, o, g)
                .collect::<crate::Result<Vec<_>>>()
                .unwrap();
            let actual = source
                .matching_query_quads(s, p, o, g)
                .map(|row| row.map(owned))
                .collect::<crate::Result<Vec<_>>>()
                .unwrap();
            assert_eq!(actual, expected, "mask={mask}");
        }
    }
}

#[test]
fn query_rows_preserve_all_masks_exact_order_and_composed_graph_domains() {
    let mut data = Dataset::new();
    let mut generalized = GeneralizedDataset::new();
    let mut shapes = Dataset::new();
    shapes.insert(&Quad::new(
        node("shape"),
        node("p"),
        Literal::new_simple_literal("shape-word"),
        GraphName::DefaultGraph,
    ));
    for graph in [
        GraphName::DefaultGraph,
        node("g").into(),
        BlankNode::new_unchecked("g").into(),
    ] {
        data.insert(&Quad::new(
            node("s"),
            node("p"),
            Literal::new_simple_literal("value"),
            graph.clone(),
        ));
        data.insert(&Quad::new(node("t"), node("q"), node("o"), graph.clone()));
        generalized.insert(&GeneralizedQuad::new(
            Literal::new_simple_literal("s"),
            Literal::new_simple_literal("p"),
            Literal::new_simple_literal("o"),
            graph,
        ));
    }
    check_masks(&data);
    check_masks(&generalized);
    let union = UnionGraph::new(&data);
    check_masks(&union);
    check_masks(&UnionGraph::new(&union));
    check_masks(&ScopedGraph::new(&data, node("g").into()));
    check_masks(&OverlayGraph::new(&union, &shapes, node("shapes").into()).unwrap());
    check_masks(&UnionGraph::new(&generalized));
}

#[test]
fn borrowed_view_rows_outlive_temporary_pattern_words_and_keep_source_buffers() {
    let mut data = Dataset::new();
    let value = Literal::new_simple_literal("dictionary-value".repeat(8192));
    for graph in [GraphName::DefaultGraph, node("g").into()] {
        data.insert(&Quad::new(node("s"), node("p"), value.clone(), graph));
    }
    let TermRef::Literal(expected) = data.iter().next().unwrap().object else {
        panic!("literal")
    };
    let union = UnionGraph::new(&data);
    let iterator = {
        let temporary_subject = node("s");
        let temporary_predicate = node("p");
        let temporary_object = value.clone();
        union.matching_query_quads(
            Some(temporary_subject.as_ref().into()),
            Some(temporary_predicate.as_ref().into()),
            Some(temporary_object.as_ref().into()),
            Some(GraphNameRef::DefaultGraph),
        )
    };
    let rows = iterator.collect::<crate::Result<Vec<_>>>().unwrap();
    assert_eq!(rows.len(), 1);
    let TermCow::Borrowed(TermRef::Literal(actual)) = rows[0].object else {
        panic!("borrowed dictionary literal required")
    };
    assert_eq!(actual.value().as_ptr(), expected.value().as_ptr());
    assert!(rows[0].graph_name.is_none());
    let scoped = ScopedGraph::new(&data, node("g").into());
    let row = scoped
        .matching_query_quads(None, None, None, Some(GraphNameRef::DefaultGraph))
        .next()
        .unwrap()
        .unwrap();
    assert!(matches!(row.object, TermCow::Borrowed(_)));
    assert!(row.graph_name.is_none());
}

#[test]
fn storage_query_rows_keep_owned_decoding_pending_state_and_snapshot_versions() {
    let store = Store::new().unwrap();
    let before = store.read_snapshot();
    let row = GeneralizedQuad::new(
        Literal::new_simple_literal("s"),
        Literal::new_simple_literal("p"),
        Literal::new_simple_literal("word".repeat(8192)),
        node("g"),
    );
    let mut tx = store.start_transaction().unwrap();
    tx.insert_generalized(&row);
    let actual = tx
        .matching_query_quads(None, None, None, None)
        .next()
        .unwrap()
        .unwrap();
    assert!(matches!(actual.object, TermCow::Owned(_)));
    assert_eq!(owned(actual), row);
    check_masks(&tx);
    assert_eq!(
        before.matching_query_quads(None, None, None, None).count(),
        0
    );
    tx.commit().unwrap();
    assert_eq!(
        before.matching_query_quads(None, None, None, None).count(),
        0
    );
    let current = store.read_snapshot();
    let actual = current
        .matching_query_quads(None, None, None, None)
        .next()
        .unwrap()
        .unwrap();
    assert!(matches!(actual.object, TermCow::Owned(_)));
    assert_eq!(owned(actual), row);
    check_masks(&current);
}

#[test]
fn query_transport_layouts_keep_the_existing_adapter_fields() {
    struct PreviousFields<'a> {
        source: &'a Dataset,
        functions: HashMap<NamedNode, ScopedCustomFunction<'a>>,
    }
    let source = Dataset::new();
    let old = PreviousFields {
        source: &source,
        functions: HashMap::new(),
    };
    let current = QueryGraph::new(&source);
    assert_eq!(std::mem::size_of_val(&old), std::mem::size_of_val(&current));
    assert_eq!(
        std::mem::align_of_val(&old),
        std::mem::align_of_val(&current)
    );
    assert!(old.source.is_empty() && old.functions.is_empty());
    assert_eq!(
        std::mem::size_of::<UnionGraph<'_, Dataset>>(),
        std::mem::size_of::<&Dataset>()
    );
    println!(
        "QUERY_LAYOUT term={} cow={} old_row={} query_row={} adapter={} union={} scoped={} overlay={} term_align={} cow_align={}",
        std::mem::size_of::<Term>(),
        std::mem::size_of::<TermCow<'_>>(),
        std::mem::size_of::<InternalQuad<Term>>(),
        std::mem::size_of::<InternalQuad<TermCow<'_>>>(),
        std::mem::size_of_val(&current),
        std::mem::size_of::<UnionGraph<'_, Dataset>>(),
        std::mem::size_of::<ScopedGraph<'_, Dataset>>(),
        std::mem::size_of::<OverlayGraph<'_, Dataset, Dataset>>(),
        std::mem::align_of::<Term>(),
        std::mem::align_of::<TermCow<'_>>()
    );
}
