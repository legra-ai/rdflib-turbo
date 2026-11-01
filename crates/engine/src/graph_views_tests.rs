use oxigraph::model::*;
use oxigraph::store::Store;

use crate::graph::objects;
use crate::{GraphRead, OverlayGraph, UnionGraph};

fn node(name: &str) -> NamedNode {
    NamedNode::new(format!("urn:{name}")).unwrap()
}

fn check_union(source: &impl GraphRead) {
    let view = UnionGraph::new(source);
    let values = objects(&view, &node("a").into(), &node("p")).unwrap();
    assert_eq!(values, vec![node("b").into()]);
    assert_eq!(
        view.matching_quads(None, None, None, Some(GraphNameRef::DefaultGraph))
            .count(),
        2
    );
    assert_eq!(
        view.matching_quads(None, None, None, Some(node("g1").as_ref().into()))
            .count(),
        1
    );
    assert_eq!(view.matching_quads(None, None, None, None).count(), 5);
    // Wrapping an already projected view does not add duplicate default facts.
    assert_eq!(
        UnionGraph::new(&view)
            .matching_quads(None, None, None, None)
            .count(),
        5
    );
}

#[test]
fn union_streams_distinct_default_facts_and_preserves_named_contexts() {
    let mut data = Dataset::new();
    for graph in [
        GraphName::DefaultGraph,
        node("g1").into(),
        node("g2").into(),
    ] {
        data.insert(&Quad::new(node("a"), node("p"), node("b"), graph));
    }
    data.insert(&Quad::new(node("c"), node("p"), node("d"), node("g2")));
    check_union(&data);
    let store = Store::new().unwrap();
    let mut transaction = store.start_transaction().unwrap();
    for quad in data.iter() {
        transaction.insert(quad);
    }
    check_union(&transaction);
    assert!(store.is_empty().unwrap());
}

#[test]
fn overlay_keeps_shapes_out_of_the_default_graph_and_preserves_base_contexts() {
    let mut data = Dataset::new();
    data.insert(&Quad::new(
        node("a"),
        node("p"),
        node("b"),
        GraphName::DefaultGraph,
    ));
    let mut shapes = Dataset::new();
    for graph in [GraphName::DefaultGraph, node("source").into()] {
        shapes.insert(&Quad::new(node("S"), node("path"), node("p"), graph));
    }
    let view = OverlayGraph::new(&data, &shapes, node("shapes").into()).unwrap();
    assert_eq!(view.matching_quads(None, None, None, None).count(), 2);
    assert_eq!(
        view.matching_quads(None, None, None, Some(GraphNameRef::DefaultGraph))
            .count(),
        1
    );
    assert_eq!(
        view.matching_quads(None, None, None, Some(node("shapes").as_ref().into()))
            .count(),
        1
    );
    assert_eq!(
        view.matching_quads(None, None, None, Some(node("source").as_ref().into()))
            .count(),
        0
    );
    assert_eq!(data.len(), 1);
    assert_eq!(shapes.len(), 2);
    assert!(view.contains_named_graph(&node("shapes").into()).unwrap());
}

#[test]
fn overlay_rejects_existing_graphs_including_empty_transaction_graphs() {
    let store = Store::new().unwrap();
    let mut transaction = store.start_transaction().unwrap();
    transaction.insert_named_graph(&node("reserved"));
    let shapes = Dataset::new();
    assert!(OverlayGraph::new(&transaction, &shapes, node("reserved").into()).is_err());
    let view = OverlayGraph::new(&transaction, &shapes, node("fresh").into()).unwrap();
    assert!(view.contains_named_graph(&node("reserved").into()).unwrap());
    assert!(view.contains_named_graph(&node("fresh").into()).unwrap());
    assert_eq!(view.matching_quads(None, None, None, None).count(), 0);
}

struct FallibleTail(std::cell::Cell<bool>);

impl GraphRead for FallibleTail {
    fn selected_focus_source(&self) -> Option<&dyn crate::SelectedFocusSource> {
        None
    }
    fn selected_value_source(&self) -> Option<&dyn crate::SelectedValueSource> {
        None
    }

    fn boolean_object_state(
        &self,
        _row: &oxigraph::model::GeneralizedQuad,
    ) -> crate::Result<Option<crate::BuiltinBooleanState>> {
        Ok(None)
    }

    fn matching_quads<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        _: Option<TermRef<'_>>,
        _: Option<TermRef<'_>>,
        _: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = crate::Result<GeneralizedQuad>> + use<'a> {
        let exact = subject.is_some();
        std::iter::once(Ok(GeneralizedQuad::new(
            node("a"),
            node("p"),
            node("b"),
            node("g"),
        )))
        .chain(
            std::iter::once_with(move || {
                self.0.set(true);
                Err(crate::Error::Runtime("late read error".into()))
            })
            .take(usize::from(!exact)),
        )
    }

    fn named_graphs(&self) -> impl Iterator<Item = crate::Result<NamedOrBlankNode>> + '_ {
        std::iter::once(Err(crate::Error::Runtime("graph metadata error".into())))
    }

    fn contains_named_graph(&self, _: &NamedOrBlankNode) -> crate::Result<bool> {
        Err(crate::Error::Runtime("graph metadata error".into()))
    }
}

#[test]
fn views_yield_before_reading_the_tail_and_propagate_storage_errors() {
    let source = FallibleTail(std::cell::Cell::new(false));
    let view = UnionGraph::new(&source);
    let mut values = view.matching_quads(None, None, None, Some(GraphNameRef::DefaultGraph));
    assert!(values.next().unwrap().is_ok());
    assert!(!source.0.get());
    assert!(
        values
            .next()
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("late read error")
    );
    assert!(source.0.get());
    assert!(OverlayGraph::new(&source, &Dataset::new(), node("shapes").into()).is_err());
}

#[test]
fn selected_graph_is_a_logical_default_and_cannot_observe_other_contexts() {
    for physical in [
        GraphName::DefaultGraph,
        node("selected").into(),
        BlankNode::new("selected").unwrap().into(),
    ] {
        let store = Store::new().unwrap();
        let mut transaction = store.start_transaction().unwrap();
        transaction.insert(&Quad::new(
            node("s"),
            node("p"),
            node("wanted"),
            physical.clone(),
        ));
        transaction.insert(&Quad::new(
            node("s"),
            node("p"),
            node("hidden"),
            node("other"),
        ));
        let view = crate::ScopedGraph::new(&transaction, physical);
        let quads = view
            .matching_quads(None, None, None, None)
            .collect::<crate::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(
            quads,
            vec![GeneralizedQuad::new(
                node("s"),
                node("p"),
                node("wanted"),
                GraphName::DefaultGraph
            )]
        );
        assert_eq!(
            view.matching_quads(None, None, None, Some(GraphNameRef::DefaultGraph))
                .count(),
            1
        );
        assert_eq!(
            view.matching_quads(None, None, None, Some(node("other").as_ref().into()))
                .count(),
            0
        );
        assert_eq!(view.named_graphs().count(), 0);
        assert!(!view.contains_named_graph(&node("other").into()).unwrap());
    }
}
