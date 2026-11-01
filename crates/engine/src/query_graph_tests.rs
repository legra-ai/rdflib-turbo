//! SPARQL reads the same borrowed logical dataset, including graph metadata.
use std::cell::Cell;

use oxigraph::model::*;
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;
use spareval::QueryableDataset;

use crate::{GraphRead, OverlayGraph, QueryGraph, UnionGraph};

fn node(name: &str) -> NamedNode {
    NamedNode::new(format!("urn:{name}")).unwrap()
}

fn ask(source: &impl GraphRead, query: &str) -> bool {
    let result = SparqlEvaluator::new()
        .parse_query(query)
        .unwrap()
        .on_queryable_dataset(QueryGraph::new(source))
        .execute()
        .unwrap();
    let QueryResults::Boolean(value) = result else {
        panic!("expected ASK")
    };
    value
}

#[test]
fn queries_preserve_union_scope_overlay_isolation_and_pending_changes() {
    let store = Store::new().unwrap();
    let original = Quad::new(node("old"), node("p"), node("o"), GraphName::DefaultGraph);
    store.insert(&original).unwrap();
    let mut tx = store.start_transaction().unwrap();
    tx.remove(&original);
    for graph in [GraphName::DefaultGraph, node("g").into()] {
        tx.insert(&Quad::new(node("s"), node("p"), node("o"), graph));
    }
    let mut shapes = Dataset::new();
    shapes.insert(QuadRef::new(
        node("shape").as_ref(),
        node("p").as_ref(),
        node("o").as_ref(),
        GraphNameRef::DefaultGraph,
    ));
    let union = UnionGraph::new(&tx);
    let view = OverlayGraph::new(&union, &shapes, node("shapes").into()).unwrap();
    assert!(ask(
        &view,
        "ASK { { SELECT (COUNT(*) AS ?n) WHERE { ?s ?p ?o } } FILTER(?n = 1) }"
    ));
    assert!(ask(
        &view,
        "ASK { GRAPH <urn:shapes> { <urn:shape> ?p ?o } }"
    ));
    assert!(ask(
        &view,
        "ASK { GRAPH ?g { <urn:s> ?p ?o } FILTER(?g = <urn:g>) }"
    ));
    assert!(!ask(&view, "ASK { <urn:shape> ?p ?o }"));
    assert!(!ask(&view, "ASK { <urn:old> ?p ?o }"));
    assert!(ask(&view, "ASK { <urn:s> <urn:p>+ <urn:o> }"));
    drop(tx);
    assert!(store.contains(&original).unwrap());
    assert_eq!(store.len().unwrap(), 1);
}

#[test]
fn graph_variables_include_empty_native_and_overlay_graphs_once() {
    let store = Store::new().unwrap();
    let mut tx = store.start_transaction().unwrap();
    tx.insert_named_graph(node("empty").as_ref());
    for subject in ["s", "t"] {
        tx.insert(&Quad::new(node(subject), node("p"), node("o"), node("g")));
    }
    let shapes = Dataset::new();
    let union = UnionGraph::new(&tx);
    let view = OverlayGraph::new(&union, &shapes, node("shapes").into()).unwrap();
    assert!(ask(
        &view,
        "ASK { { SELECT (COUNT(*) AS ?n) WHERE { GRAPH ?g {} } } FILTER(?n = 3) }"
    ));
    assert!(ask(&view, "ASK { GRAPH <urn:empty> {} }"));
    assert!(ask(&view, "ASK { GRAPH <urn:shapes> {} }"));
    assert!(!ask(&view, "ASK { GRAPH <urn:missing> {} }"));
    let mut dataset = Dataset::new();
    for subject in ["s", "t"] {
        dataset.insert(QuadRef::new(
            node(subject).as_ref(),
            node("p").as_ref(),
            node("o").as_ref(),
            node("g").as_ref(),
        ));
    }
    assert!(ask(
        &dataset,
        "ASK { { SELECT (COUNT(*) AS ?n) WHERE { GRAPH ?g {} } } FILTER(?n = 1) }"
    ));
}

#[test]
fn indexed_patterns_own_temporary_filters_and_preserve_exact_terms() {
    let mut dataset = Dataset::new();
    let value = Literal::new_typed_literal("01", node("datatype"));
    let blank = BlankNode::new("graph").unwrap();
    for graph in [
        GraphName::DefaultGraph,
        node("g").into(),
        blank.clone().into(),
    ] {
        dataset.insert(QuadRef::new(
            node("s").as_ref(),
            node("p").as_ref(),
            value.as_ref(),
            graph.as_ref(),
        ));
        dataset.insert(QuadRef::new(
            node("t").as_ref(),
            node("q").as_ref(),
            node("o").as_ref(),
            graph.as_ref(),
        ));
    }
    for bits in 0..16 {
        let subject = node("s");
        let predicate = node("p");
        let object = Term::from(value.clone());
        let graph = GraphName::from(blank.clone());
        let subject = (bits & 1 != 0).then(|| subject.as_ref().into());
        let predicate = (bits & 2 != 0).then(|| predicate.as_ref());
        let object = (bits & 4 != 0).then(|| object.as_ref());
        let graph = (bits & 8 != 0).then(|| graph.as_ref());
        // bounded: six fixture quads; compare all combinations of indexed filters.
        let actual = dataset
            .matching_quads(
                subject.map(Into::into),
                predicate.map(Into::into),
                object,
                graph,
            )
            .collect::<crate::Result<Vec<_>>>()
            .unwrap();
        let expected = dataset
            .quads_for_pattern(subject, predicate, object, graph)
            .map(|quad| GeneralizedQuad::from(quad.into_owned()))
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "pattern {bits}");
    }
    let matches = dataset.matching_quads(
        Some(node("s").as_ref().into()),
        Some(node("p").as_ref().into()),
        Some(value.as_ref().into()),
        Some(blank.as_ref().into()),
    );
    let quad = matches.into_iter().next().unwrap().unwrap();
    assert_eq!(quad.object, value.into());
    let adapter = QueryGraph::new(&dataset);
    assert_eq!(adapter.internal_named_graphs().count(), 2);
    assert!(
        adapter
            .contains_internal_graph_name(&adapter.internalize_term(blank.clone().into()).unwrap())
            .unwrap()
    );
    assert_eq!(
        adapter
            .internal_quads_for_pattern(
                None,
                None,
                None,
                Some(Some(&adapter.internalize_term(blank.into()).unwrap()))
            )
            .count(),
        2
    );
    let QueryResults::Solutions(mut rows) = SparqlEvaluator::new()
        .parse_query("SELECT ?o WHERE { <urn:s> <urn:p> ?o }")
        .unwrap()
        .on_queryable_dataset(adapter)
        .execute()
        .unwrap()
    else {
        panic!("expected SELECT")
    };
    assert_eq!(rows.next().unwrap().unwrap().get("o"), Some(&quad.object));
}

#[test]
fn invalid_pattern_term_kinds_cannot_match() {
    let dataset = Dataset::new();
    let adapter = QueryGraph::new(&dataset);
    let invalid = adapter
        .internalize_term(Literal::new_simple_literal("invalid").into())
        .unwrap();
    assert_eq!(
        adapter
            .internal_quads_for_pattern(Some(&invalid), None, None, Some(None))
            .count(),
        0
    );
    assert_eq!(
        adapter
            .internal_quads_for_pattern(None, Some(&invalid), None, Some(None))
            .count(),
        0
    );
    assert_eq!(
        adapter
            .internal_quads_for_pattern(None, None, None, Some(Some(&invalid)))
            .count(),
        0
    );
    assert!(!adapter.contains_internal_graph_name(&invalid).unwrap());
}

struct FallibleGraph {
    tail_read: Cell<bool>,
}

impl GraphRead for FallibleGraph {
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
        _: Option<TermRef<'_>>,
        _: Option<TermRef<'_>>,
        _: Option<TermRef<'_>>,
        _: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = crate::Result<GeneralizedQuad>> + use<'a> {
        std::iter::once(Ok(GeneralizedQuad::new(
            node("s"),
            node("p"),
            node("o"),
            node("g"),
        )))
        .chain(std::iter::once_with(|| {
            self.tail_read.set(true);
            Err(crate::Error::Runtime("quad tail read failed".into()))
        }))
    }

    fn named_graphs(&self) -> impl Iterator<Item = crate::Result<NamedOrBlankNode>> + '_ {
        std::iter::once(Ok(node("g").into())).chain(std::iter::once_with(|| {
            self.tail_read.set(true);
            Err(crate::Error::Runtime("graph tail read failed".into()))
        }))
    }

    fn contains_named_graph(&self, _: &NamedOrBlankNode) -> crate::Result<bool> {
        Err(crate::Error::Runtime("graph existence read failed".into()))
    }
}

#[test]
fn reads_and_graph_metadata_are_lazy_and_errors_survive_query_evaluation() {
    let source = FallibleGraph {
        tail_read: Cell::new(false),
    };
    let adapter = QueryGraph::new(&source);
    let mut quads = adapter.internal_quads_for_pattern(None, None, None, None);
    assert!(quads.next().unwrap().is_ok());
    assert!(!source.tail_read.get());
    assert!(
        quads
            .next()
            .unwrap()
            .err()
            .unwrap()
            .to_string()
            .contains("quad tail read failed")
    );
    source.tail_read.set(false);
    let mut graphs = adapter.internal_named_graphs();
    assert!(graphs.next().unwrap().is_ok());
    assert!(!source.tail_read.get());
    assert!(
        graphs
            .next()
            .unwrap()
            .err()
            .unwrap()
            .to_string()
            .contains("graph tail read failed")
    );
    assert!(
        adapter
            .contains_internal_graph_name(&adapter.internalize_term(node("g").into()).unwrap())
            .unwrap_err()
            .to_string()
            .contains("existence read failed")
    );
    for (query, message) in [
        (
            "SELECT * WHERE { GRAPH ?g { ?s ?p ?o } }",
            "quad tail read failed",
        ),
        ("SELECT * WHERE { GRAPH ?g {} }", "graph tail read failed"),
    ] {
        let result = SparqlEvaluator::new()
            .parse_query(query)
            .unwrap()
            .on_queryable_dataset(QueryGraph::new(&source))
            .execute()
            .unwrap();
        let QueryResults::Solutions(rows) = result else {
            panic!("expected SELECT")
        };
        // bounded: one successful fixture row followed by its injected storage error.
        let error = rows
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap_err();
        assert!(error.to_string().contains(message), "{error}");
    }
}
