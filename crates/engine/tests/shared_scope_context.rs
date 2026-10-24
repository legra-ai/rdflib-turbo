use pyshacl_turbo_engine::oxigraph::model::*;
use pyshacl_turbo_engine::oxigraph::store::Store;
use pyshacl_turbo_engine::{QueryGraph, ScopedGraph};
use spareval::{QueryEvaluator, QueryResults};

#[test]
fn initial_context_queries_borrow_the_selected_pending_graph() {
    let store = Store::new().unwrap();
    let mut transaction = store.start_transaction().unwrap();
    let selected = NamedNode::new("urn:selected").unwrap();
    let subject = NamedNode::new("urn:subject").unwrap();
    let predicate = NamedNode::new("urn:value").unwrap();
    let lexical = Literal::new_typed_literal(
        "01",
        NamedNode::new("http://www.w3.org/2001/XMLSchema#integer").unwrap(),
    );
    transaction.insert(&Quad::new(
        subject.clone(),
        predicate.clone(),
        lexical.clone(),
        selected.clone(),
    ));
    transaction.insert(&Quad::new(
        subject.clone(),
        predicate,
        Literal::new_simple_literal("outside"),
        NamedNode::new("urn:outside").unwrap(),
    ));
    let source = ScopedGraph::new(&transaction, selected.into());
    let query = spargebra::SparqlParser::new()
        .with_rdflib_query_context()
        .parse_query("SELECT ?value WHERE { ?s <urn:value> ?value }")
        .unwrap();
    let evaluator = QueryEvaluator::new()
        .with_initial_bindings([(Variable::new("s").unwrap(), subject.into())]);
    let QueryResults::Solutions(mut rows) = evaluator
        .prepare(&query)
        .execute(QueryGraph::new(&source))
        .unwrap()
    else {
        panic!("expected solutions")
    };
    let row = rows.next().unwrap().unwrap();
    assert_eq!(row.get("value"), Some(&lexical.into()));
    assert!(row.get("s").is_none());
    assert!(rows.next().is_none());
    assert!(store.is_empty().unwrap());
}

#[test]
fn initial_context_preserves_exact_literals_of_the_borrowed_owner() {
    let mut dataset = Dataset::new();
    let selected = NamedNode::new("urn:selected").unwrap();
    let subject = NamedNode::new("urn:subject").unwrap();
    let lexical = Literal::new_typed_literal(
        "01",
        NamedNode::new("http://www.w3.org/2001/XMLSchema#integer").unwrap(),
    );
    dataset.insert(&Quad::new(
        subject.clone(),
        NamedNode::new("urn:value").unwrap(),
        lexical.clone(),
        selected.clone(),
    ));
    let source = ScopedGraph::new(&dataset, selected.into());
    let query = spargebra::SparqlParser::new()
        .with_rdflib_query_context()
        .parse_query("SELECT ?value WHERE { ?s <urn:value> ?value }")
        .unwrap();
    let evaluator = QueryEvaluator::new()
        .with_initial_bindings([(Variable::new("s").unwrap(), subject.into())]);
    let QueryResults::Solutions(mut rows) = evaluator
        .prepare(&query)
        .execute(QueryGraph::new(&source))
        .unwrap()
    else {
        panic!("expected solutions")
    };
    assert_eq!(
        rows.next().unwrap().unwrap().get("value"),
        Some(&lexical.into())
    );
    assert!(rows.next().is_none());
}
