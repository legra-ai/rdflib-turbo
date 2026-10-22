//! Compare borrowed query rows with the old materialized union.
use oxigraph::sparql::{QueryResults, SparqlEvaluator};

use super::*;
use crate::inference::backend::Graph;

fn rows(results: QueryResults<'_>) -> Vec<Vec<(String, String)>> {
    let QueryResults::Solutions(solutions) = results else {
        panic!("SELECT must return solution rows");
    };
    // bounded: every query runs against this test's fixed five-quad fixture.
    let mut rows: Vec<_> = solutions
        .map(|solution| {
            solution
                .unwrap()
                .iter()
                .map(|(variable, term)| (variable.to_string(), term.to_string()))
                .collect::<Vec<_>>()
        })
        .collect();
    rows.sort();
    rows
}

#[test]
fn borrowed_queries_preserve_contexts_exact_terms_and_invalid_bound_patterns() {
    let mut source = Dataset::new();
    let subject = NamedNode::new_unchecked("urn:s");
    let predicate = NamedNode::new_unchecked("urn:p");
    for graph in [
        GraphName::DefaultGraph,
        NamedNode::new_unchecked("urn:source").into(),
        BlankNode::new("source").unwrap().into(),
    ] {
        for object in [
            Literal::new_simple_literal("word"),
            Literal::new_typed_literal(
                "word",
                NamedNode::new_unchecked("http://www.w3.org/2001/XMLSchema#string"),
            ),
        ] {
            source.insert(&Quad::new(
                subject.clone(),
                predicate.clone(),
                object,
                graph.clone(),
            ));
        }
    }
    let mut staged = StagedDataset::new(&source, NamedNode::new_unchecked("urn:output").into());
    // A publication already in a different context must not duplicate the logical union.
    staged.insert(Quad::new(
        subject.clone(),
        predicate.clone(),
        Literal::new_simple_literal("word"),
        GraphName::DefaultGraph,
    ));
    staged.insert(Quad::new(
        subject,
        predicate,
        Literal::new_simple_literal("derived"),
        GraphName::DefaultGraph,
    ));
    // bounded: a frozen union of the six synthetic source quads and two additions.
    let mut expected = crate::graph::with_union(&source);
    expected.extend(staged.output.iter());
    for query in [
        "SELECT ?s ?p ?o WHERE { ?s ?p ?o }",
        "SELECT ?s ?p ?o ?g WHERE { GRAPH ?g { ?s ?p ?o } }",
        "SELECT ?s ?p ?o WHERE { GRAPH <urn:source> { ?s ?p ?o } }",
        "SELECT ?s ?p ?o WHERE { GRAPH <urn:output> { ?s ?p ?o } }",
        "SELECT ?s ?p ?o WHERE { VALUES ?s { 1 } ?s ?p ?o }",
        "SELECT ?s ?p ?o WHERE { VALUES ?p { 1 } ?s ?p ?o }",
        "SELECT ?s ?p ?o WHERE { VALUES ?g { 1 } GRAPH ?g { ?s ?p ?o } }",
        "SELECT ?o WHERE { <urn:s> <urn:p> ?o FILTER(sameTerm(?o, \"word\")) }",
        "SELECT ?o WHERE { <urn:s> <urn:p> ?o FILTER(sameTerm(?o, \"word\"^^<http://www.w3.org/2001/XMLSchema#string>)) }",
    ] {
        let prepared = SparqlEvaluator::new().parse_query(query).unwrap();
        let actual = rows(staged.query(prepared.clone()).unwrap());
        let expected = rows(prepared.on_queryable_dataset(&expected).execute().unwrap());
        assert_eq!(actual, expected, "{query}");
    }
}
