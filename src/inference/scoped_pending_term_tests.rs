//! Full-Term inference publication has an explicit policy, separate from storage.
use std::cell::Cell;
use std::collections::HashSet;

use oxigraph::model::*;
use oxigraph::store::Store;
use pyshacl_turbo_engine::GraphRead;

use super::super::backend::Graph;
use super::super::{BorrowedInference, HeadDisposition, Pending};
use super::ScopedGraph;

#[test]
fn pending_dispositions_preserve_existing_literal_predicates_and_undo_publications() {
    let source: GraphName = NamedNode::new("urn:scope").unwrap().into();
    let destination: GraphName = BlankNode::new("urn:scope").unwrap().into();
    let store = Store::new().unwrap();
    let existing_source = GeneralizedQuad::new(
        Literal::new_simple_literal("source"),
        Literal::new_simple_literal("existing-predicate"),
        Literal::new_simple_literal("value"),
        source.clone(),
    );
    let mut existing_destination = existing_source.clone();
    existing_destination.subject = Literal::new_simple_literal("destination").into();
    existing_destination.graph_name = destination.clone();
    for row in [&existing_source, &existing_destination] {
        store.insert_generalized(row).unwrap();
    }
    let literal_subject = GeneralizedQuad::new(
        Literal::new_simple_literal("derived-subject"),
        NamedNode::new("urn:predicate").unwrap(),
        Literal::new_simple_literal("value"),
        GraphName::DefaultGraph,
    );
    let blank_predicate = GeneralizedQuad::new(
        BlankNode::new("derived-subject").unwrap(),
        BlankNode::new("derived-predicate").unwrap(),
        Literal::new_simple_literal("value"),
        GraphName::DefaultGraph,
    );
    {
        let mut transaction = store.start_transaction().unwrap();
        let mut graph = ScopedGraph {
            transaction: &mut transaction,
            source,
            destination: destination.clone(),
        };
        let mut pending = Pending::default();
        for row in [&existing_source, &existing_destination] {
            let mut head = row.clone();
            head.graph_name = GraphName::DefaultGraph;
            assert_eq!(
                pending.push(&graph, head).unwrap(),
                HeadDisposition::AlreadyPresent
            );
        }
        let mut refused = existing_source.clone();
        refused.subject = Literal::new_simple_literal("new-head").into();
        refused.graph_name = GraphName::DefaultGraph;
        assert_eq!(
            pending.push(&graph, refused).unwrap(),
            HeadDisposition::RefusedLiteralPredicate
        );
        assert!(pending.is_empty());
        for row in [&literal_subject, &blank_predicate] {
            assert_eq!(
                pending.push(&graph, row.clone()).unwrap(),
                HeadDisposition::Accepted
            );
            assert_eq!(
                pending.push(&graph, row.clone()).unwrap(),
                HeadDisposition::AlreadyPresent
            );
            assert!(!graph.contains_destination(row.into()).unwrap());
        }
        for row in pending {
            graph.insert(row).unwrap();
        }
        assert!(
            graph
                .contains_destination((&literal_subject).into())
                .unwrap()
        );
        assert!(
            graph
                .contains_destination((&blank_predicate).into())
                .unwrap()
        );
        assert_eq!(
            graph
                .quads(None, None, None)
                .collect::<crate::Result<HashSet<_>>>()
                .unwrap(),
            HashSet::from([existing_source.clone()])
        );
    }
    assert!(store.contains_generalized(&existing_source).unwrap());
    assert!(store.contains_generalized(&existing_destination).unwrap());
    for mut row in [literal_subject, blank_predicate] {
        row.graph_name = destination.clone();
        assert!(!store.contains_generalized(&row).unwrap());
    }
}

#[derive(Default)]
struct Source {
    fails: Cell<bool>,
}
impl GraphRead for Source {
    fn matching_quads<'a>(
        &'a self,
        _subject: Option<TermRef<'_>>,
        _predicate: Option<TermRef<'_>>,
        _object: Option<TermRef<'_>>,
        _graph: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = pyshacl_turbo_engine::Result<GeneralizedQuad>> + use<'a> {
        self.fails
            .get()
            .then(|| {
                Err(pyshacl_turbo_engine::Error::Runtime(
                    "source-first read failure".into(),
                ))
            })
            .into_iter()
    }
    fn named_graphs(
        &self,
    ) -> impl Iterator<Item = pyshacl_turbo_engine::Result<NamedOrBlankNode>> + '_ {
        std::iter::empty()
    }
    fn contains_named_graph(
        &self,
        _graph: &NamedOrBlankNode,
    ) -> pyshacl_turbo_engine::Result<bool> {
        Ok(false)
    }
}

#[test]
fn full_term_overlay_hit_does_not_hide_source_error_or_enter_pending() {
    let source = Source::default();
    let row = GeneralizedQuad::new(
        Literal::new_simple_literal("subject"),
        BlankNode::new("predicate").unwrap(),
        Literal::new_simple_literal("object"),
        GraphName::DefaultGraph,
    );
    let mut graph =
        BorrowedInference::new(&source, NamedNode::new("urn:destination").unwrap().into());
    graph.insert(row.clone()).unwrap();
    source.fails.set(true);
    let mut pending = Pending::default();
    assert!(
        pending
            .push(&graph, row)
            .unwrap_err()
            .to_string()
            .contains("source-first")
    );
    assert!(pending.is_empty());
}
