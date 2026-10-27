//! Stable-first and lazy errors are query contracts, not dataset ordering guesses.
use crate::{GraphRead, QueryGraph, UnionGraph};
use oxigraph::model::*;
use oxigraph::sparql::{CancellationToken, QueryResults, SparqlEvaluator};

fn node(word: &str) -> NamedNode {
    NamedNode::new(format!("urn:view:{word}")).unwrap()
}

struct NamedFirst(Dataset);
impl GraphRead for NamedFirst {
    fn matching_quads<'a>(
        &'a self,
        s: Option<TermRef<'_>>,
        p: Option<TermRef<'_>>,
        o: Option<TermRef<'_>>,
        g: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = crate::Result<GeneralizedQuad>> + use<'a> {
        let rows: Box<dyn Iterator<Item = _>> = if g.is_some() {
            Box::new(self.0.matching_quads(s, p, o, g))
        } else {
            Box::new(
                self.0
                    .matching_quads(s, p, o, Some(node("g").as_ref().into()))
                    .chain(
                        self.0
                            .matching_quads(s, p, o, Some(GraphNameRef::DefaultGraph)),
                    ),
            )
        };
        rows
    }
    fn named_graphs(&self) -> impl Iterator<Item = crate::Result<NamedOrBlankNode>> + '_ {
        GraphRead::named_graphs(&self.0)
    }
    fn contains_named_graph(&self, g: &NamedOrBlankNode) -> crate::Result<bool> {
        self.0.contains_named_graph(g)
    }
}

#[test]
fn query_union_accepts_a_named_first_source_and_nested_union_without_duplication() {
    let mut data = Dataset::new();
    for g in [GraphName::DefaultGraph, node("g").into()] {
        data.insert(&Quad::new(node("s"), node("p"), node("o"), g));
    }
    let source = NamedFirst(data);
    let first = source
        .matching_quads(None, None, None, None)
        .next()
        .unwrap()
        .unwrap();
    assert_eq!(first.graph_name, GraphName::from(node("g")));
    let union = UnionGraph::new(&source);
    for result in [
        union
            .matching_query_quads(None, None, None, Some(GraphNameRef::DefaultGraph))
            .count(),
        UnionGraph::new(&union)
            .matching_query_quads(None, None, None, Some(GraphNameRef::DefaultGraph))
            .count(),
    ] {
        assert_eq!(result, 1);
    }
    let result = SparqlEvaluator::new()
        .parse_query("SELECT ?s ?p ?o WHERE { ?s ?p ?o }")
        .unwrap()
        .on_queryable_dataset(QueryGraph::new(&union))
        .execute()
        .unwrap();
    let QueryResults::Solutions(rows) = result else {
        panic!("SELECT")
    };
    let rows = rows.collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("o"), Some(&Term::from(node("o"))));
}

struct FailedRepresentative(bool);
impl GraphRead for FailedRepresentative {
    fn matching_quads<'a>(
        &'a self,
        s: Option<TermRef<'_>>,
        _: Option<TermRef<'_>>,
        _: Option<TermRef<'_>>,
        _: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = crate::Result<GeneralizedQuad>> + use<'a> {
        let rows: Box<dyn Iterator<Item = _>> = if s.is_none() {
            Box::new(std::iter::once(Ok(GeneralizedQuad::new(
                node("s"),
                node("p"),
                node("o"),
                node("g"),
            ))))
        } else if self.0 {
            Box::new(std::iter::once(Err(crate::Error::Runtime(
                "representative read failed".into(),
            ))))
        } else {
            Box::new(std::iter::empty())
        };
        rows
    }
    fn named_graphs(&self) -> impl Iterator<Item = crate::Result<NamedOrBlankNode>> + '_ {
        std::iter::empty()
    }
    fn contains_named_graph(&self, _: &NamedOrBlankNode) -> crate::Result<bool> {
        Ok(false)
    }
}

#[test]
fn query_union_preserves_first_match_errors_and_changed_source_failure() {
    for (source, message) in [
        (FailedRepresentative(true), "representative read failed"),
        (
            FailedRepresentative(false),
            "Graph changed during union read",
        ),
    ] {
        let union = UnionGraph::new(&source);
        let old = union
            .matching_quads(None, None, None, Some(GraphNameRef::DefaultGraph))
            .next()
            .unwrap()
            .err()
            .unwrap();
        let new = union
            .matching_query_quads(None, None, None, Some(GraphNameRef::DefaultGraph))
            .next()
            .unwrap()
            .err()
            .unwrap();
        assert_eq!(new.to_string(), old.to_string());
        assert!(new.to_string().contains(message));
    }
}

#[test]
fn query_union_keeps_existing_evaluator_cancellation() {
    let mut data = Dataset::new();
    data.insert(&Quad::new(node("s"), node("p"), node("o"), node("g")));
    let union = UnionGraph::new(&data);
    let token = CancellationToken::new();
    token.cancel();
    let result = SparqlEvaluator::new()
        .with_cancellation_token(token)
        .parse_query("SELECT * WHERE { ?s ?p ?o }")
        .unwrap()
        .on_queryable_dataset(QueryGraph::new(&union))
        .execute();
    match result {
        Err(error) => assert!(error.to_string().to_lowercase().contains("cancel")),
        Ok(QueryResults::Solutions(rows)) => {
            let error = rows.collect::<Result<Vec<_>, _>>().unwrap_err();
            assert!(error.to_string().to_lowercase().contains("cancel"));
        }
        _ => panic!("cancelled SELECT required"),
    }
}
