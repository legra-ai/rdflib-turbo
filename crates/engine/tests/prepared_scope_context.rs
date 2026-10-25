use pyshacl_turbo_engine::oxigraph::model::*;
use pyshacl_turbo_engine::oxigraph::store::Store;
use pyshacl_turbo_engine::{QueryGraph, ScopedGraph};
use spareval::{QueryEvaluator, QueryResults, RdfLibPreparedAlgebra};
use spargebra::Query;
use sparopt::algebra::GraphPattern;

fn prepare(source: &str) -> (Query, RdfLibPreparedAlgebra) {
    let mut query = spargebra::SparqlParser::new()
        .with_rdflib_query_context()
        .parse_query(source)
        .unwrap();
    let Query::Select { pattern, .. } = &mut query else {
        panic!("expected SELECT")
    };
    let prepared = RdfLibPreparedAlgebra::new(GraphPattern::from_rdflib_algebra(pattern));
    *pattern = prepared.source().clone();
    (query, prepared)
}

#[test]
fn prepared_plan_borrows_exact_selected_pending_terms() {
    let store = Store::new().unwrap();
    let mut transaction = store.start_transaction().unwrap();
    let selected = BlankNode::new("selected").unwrap();
    let subject = NamedNode::new("urn:s").unwrap();
    let value = Literal::new_typed_literal(
        "01",
        NamedNode::new("http://www.w3.org/2001/XMLSchema#integer").unwrap(),
    );
    for (graph, object) in [
        (GraphName::from(selected.clone()), value.clone()),
        (
            GraphName::DefaultGraph,
            Literal::new_simple_literal("outside"),
        ),
    ] {
        transaction.insert(&Quad::new(
            subject.clone(),
            NamedNode::new("urn:p").unwrap(),
            object,
            graph,
        ));
    }
    let (query, prepared) = prepare("SELECT ?value WHERE {?s <urn:p> ?value}");
    let evaluator = QueryEvaluator::new()
        .with_initial_bindings([(Variable::new("s").unwrap(), subject.into())])
        .with_rdflib_prepared_algebra(prepared);
    let source = ScopedGraph::new(&transaction, selected.into());
    let QueryResults::Solutions(mut rows) = evaluator
        .prepare(&query)
        .execute(QueryGraph::new(&source))
        .unwrap()
    else {
        panic!("expected solutions")
    };
    assert_eq!(
        rows.next().unwrap().unwrap().get("value"),
        Some(&value.into())
    );
    assert!(rows.next().is_none());
    assert!(store.is_empty().unwrap());
}

#[test]
fn prepared_plan_rejects_an_unrelated_query_tree() {
    let (_, prepared) = prepare("SELECT ?s WHERE {?s <urn:p> ?value}");
    let (other, _) = prepare("SELECT ?s WHERE {?s <urn:q> ?value}");
    let dataset = Dataset::new();
    assert!(
        QueryEvaluator::new()
            .with_rdflib_prepared_algebra(prepared)
            .prepare(&other)
            .execute(QueryGraph::new(&dataset))
            .is_err()
    );
}

#[test]
fn prepared_filter_keeps_original_exists_expression_scope() {
    let (query, prepared) =
        prepare("SELECT ?s WHERE {BIND(<urn:a> AS ?s) FILTER EXISTS {{FILTER(?s=<urn:a>)}}}");
    let dataset = Dataset::new();
    let QueryResults::Solutions(mut rows) = QueryEvaluator::new()
        .with_rdflib_prepared_algebra(prepared)
        .prepare(&query)
        .execute(QueryGraph::new(&dataset))
        .unwrap()
    else {
        panic!("expected solutions")
    };
    assert!(rows.next().is_none());
}

#[test]
fn prepared_query_configuration_is_rejected_for_updates() {
    let (_, prepared) = prepare("SELECT ?s WHERE {?s <urn:p> ?value}");
    let update = spargebra::SparqlParser::new()
        .parse_update("INSERT {?s <urn:q> ?value} WHERE {?s <urn:p> ?value}")
        .unwrap();
    let spargebra::GraphUpdateOperation::DeleteInsert {
        delete,
        insert,
        pattern,
        ..
    } = &update.operations[0]
    else {
        panic!("expected DELETE/INSERT")
    };
    let dataset = Dataset::new();
    assert!(
        QueryEvaluator::new()
            .with_rdflib_prepared_algebra(prepared)
            .prepare_delete_insert(delete.clone(), insert.clone(), None, None, pattern)
            .execute(QueryGraph::new(&dataset))
            .is_err()
    );
}
