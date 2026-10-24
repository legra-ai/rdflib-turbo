//! Existence needs one source probe, not union representative reconstruction.
use std::cell::Cell;

use oxigraph::model::*;
use oxigraph::store::Store;
use pyshacl_turbo_engine::{Error as ReadError, GraphRead};

use super::super::backend::Graph;
use super::BorrowedInference;

struct Source {
    data: Dataset,
    calls: Cell<usize>,
    fail: Cell<bool>,
}

impl GraphRead for Source {
    fn matching_quads<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = pyshacl_turbo_engine::Result<GeneralizedQuad>> + use<'a> {
        self.calls.set(self.calls.get() + 1);
        let mut fail = self.fail.get();
        let failure = std::iter::from_fn(move || {
            std::mem::take(&mut fail)
                .then(|| Err(ReadError::Runtime("boolean source failure".into())))
        });
        let (subject, valid_subject): (Option<NamedOrBlankNodeRef<'_>>, bool) = match subject {
            Some(TermRef::NamedNode(node)) => (Some(node.into()), true),
            Some(TermRef::BlankNode(node)) => (Some(node.into()), true),
            Some(_) => (None, false),
            None => (None, true),
        };
        let (predicate, valid_predicate) = match predicate {
            Some(TermRef::NamedNode(node)) => (Some(node), true),
            Some(_) => (None, false),
            None => (None, true),
        };
        failure.chain(
            self.data
                .quads_for_pattern(subject, predicate, object, graph)
                .filter(move |_| valid_subject && valid_predicate)
                .map(|quad| Ok(quad.into_owned().into())),
        )
    }
    fn named_graphs(
        &self,
    ) -> impl Iterator<Item = pyshacl_turbo_engine::Result<NamedOrBlankNode>> + '_ {
        std::iter::empty()
    }
    fn contains_named_graph(&self, _: &NamedOrBlankNode) -> pyshacl_turbo_engine::Result<bool> {
        Ok(false)
    }
}

fn row() -> Quad {
    Quad::new(
        NamedNode::new("urn:s").unwrap(),
        NamedNode::new("urn:p").unwrap(),
        Literal::new_simple_literal("value"),
        GraphName::DefaultGraph,
    )
}
fn source(data: Dataset) -> Source {
    Source {
        data,
        calls: Cell::new(0),
        fail: Cell::new(false),
    }
}

#[test]
fn borrowed_boolean_uses_one_source_probe_for_source_and_overlay_hits() {
    let mut calls = [0; 2]; // bounded: overlay-only and source-present controls.
    for (index, in_source) in [false, true].into_iter().enumerate() {
        let quad = row();
        let source = source(if in_source {
            Dataset::from_iter([&quad])
        } else {
            Dataset::new()
        });
        let mut data = BorrowedInference::new(&source, GraphName::DefaultGraph);
        if !in_source {
            data.insert(quad.clone().into()).unwrap();
        }
        source.calls.set(0);
        assert!(data.contains((&quad).into()).unwrap());
        calls[index] = source.calls.get();
    }
    assert_eq!(
        calls,
        [1, 1],
        "boolean existence reconstructed a representative"
    );
}

#[test]
fn borrowed_overlay_hits_do_not_hide_source_read_errors() {
    let source = source(Dataset::new());
    let mut data = BorrowedInference::new(&source, GraphName::DefaultGraph);
    data.insert(row().into()).unwrap();
    source.fail.set(true);
    for destination_only in [false, true] {
        source.calls.set(0);
        let result = if destination_only {
            data.contains_destination((&row()).into())
        } else {
            data.contains((&row()).into())
        };
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("boolean source failure")
        );
        assert_eq!(source.calls.get(), 1);
    }
}

#[test]
fn borrowed_boolean_preserves_source_scope_and_destination_only_semantics() {
    let store = Store::new().unwrap();
    let a = NamedNode::new("urn:source").unwrap();
    let b = NamedNode::new("urn:outside").unwrap();
    let mut transaction = store.start_transaction().unwrap();
    let mut inside = row();
    inside.graph_name = a.clone().into();
    transaction.insert(&inside);
    let mut outside = row();
    outside.subject = NamedNode::new("urn:outside-subject").unwrap().into();
    outside.graph_name = b.clone().into();
    transaction.insert(&outside);
    let selected = pyshacl_turbo_engine::ScopedGraph::new(&transaction, a.into());
    let mut data = BorrowedInference::new(&selected, NamedNode::new("urn:derived").unwrap().into());
    // ScopedGraph exposes its physical source as the logical default graph.
    let default_destination = BorrowedInference::new(&selected, GraphName::DefaultGraph);
    assert!(
        default_destination
            .contains_destination((&inside).into())
            .unwrap()
    );
    assert!(data.contains((&inside).into()).unwrap());
    assert!(!data.contains_destination((&inside).into()).unwrap());
    assert!(!data.contains((&outside).into()).unwrap());
    data.insert(outside.clone().into()).unwrap();
    assert!(data.contains((&outside).into()).unwrap());
    assert!(data.contains_destination((&outside).into()).unwrap());
}

#[cfg(feature = "allocation-profiling")]
#[test]
#[ignore = "global allocation counters require isolated execution"]
fn borrowed_overlay_boolean_does_not_materialize_overlay_lexical_terms() {
    let text = "x".repeat(64 * 1024);
    let quad = Quad::new(
        NamedNode::new(format!("urn:s:{text}")).unwrap(),
        NamedNode::new(format!("urn:p:{text}")).unwrap(),
        Literal::new_simple_literal(&text),
        GraphName::DefaultGraph,
    );
    let source = source(Dataset::new());
    let mut data = BorrowedInference::new(&source, GraphName::DefaultGraph);
    data.insert(quad.clone().into()).unwrap();
    let (present, allocated) =
        crate::allocation_tests::measure_allocated(|| data.contains((&quad).into()).unwrap());
    assert!(present);
    assert_eq!(
        allocated, 0,
        "overlay boolean cloned lexical payload or constructed union adapters"
    );
    let (present, allocated) = crate::allocation_tests::measure_allocated(|| {
        data.contains_destination((&quad).into()).unwrap()
    });
    assert!(present);
    assert_eq!(
        allocated, 0,
        "overlay destination boolean cloned lexical payload or constructed union adapters"
    );
}
