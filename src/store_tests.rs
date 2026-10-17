//! Regression gate before moving the shared runtime onto Oxigraph Store.

use oxigraph::model::{GraphName, Literal, NamedNode, Quad, Term};
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;

fn node(value: &str) -> NamedNode {
    NamedNode::new(format!("urn:{value}")).unwrap()
}

fn literal(value: &str, datatype: &str) -> Literal {
    Literal::new_typed_literal(
        value,
        NamedNode::new(format!("http://www.w3.org/2001/XMLSchema#{datatype}")).unwrap(),
    )
}

fn quad(value: Literal) -> Quad {
    Quad::new(node("s"), node("p"), value, GraphName::DefaultGraph)
}

#[test]
fn store_keeps_numeric_spellings_and_declared_datatypes() {
    let store = Store::new().unwrap();
    let values = [
        literal("01", "integer"),
        literal("1", "integer"),
        literal("4.0", "decimal"),
        literal("300", "byte"),
        literal("+1", "int"),
    ];
    for value in &values {
        store.insert(&quad(value.clone())).unwrap();
    }
    assert_eq!(store.len().unwrap(), values.len());
    for value in values {
        assert!(
            store
                .iter()
                .any(|entry| entry.unwrap().object == Term::Literal(value.clone()))
        );
    }
}

#[test]
fn transaction_abort_and_commit_preserve_exact_terms() {
    let store = Store::new().unwrap();
    store.insert(&quad(literal("01", "integer"))).unwrap();
    {
        let mut transaction = store.start_transaction().unwrap();
        transaction.insert(&quad(literal("1", "integer")));
        assert_eq!(transaction.len().unwrap(), 2);
    }
    assert_eq!(store.len().unwrap(), 1);
    let mut transaction = store.start_transaction().unwrap();
    transaction.insert(&quad(literal("1", "integer")));
    transaction.commit().unwrap();
    assert_eq!(store.len().unwrap(), 2);
}

#[test]
fn select_and_construct_keep_literal_lexical_identity() {
    let store = Store::new().unwrap();
    let value = literal("01", "integer");
    store.insert(&quad(value.clone())).unwrap();
    let QueryResults::Solutions(mut rows) = SparqlEvaluator::new()
        .parse_query("SELECT ?o WHERE { ?s ?p ?o }")
        .unwrap()
        .on_store(&store)
        .execute()
        .unwrap()
    else {
        panic!("SELECT")
    };
    assert_eq!(
        rows.next().unwrap().unwrap().get("o"),
        Some(&Term::Literal(value.clone()))
    );
    let QueryResults::Graph(mut triples) = SparqlEvaluator::new()
        .parse_query("CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }")
        .unwrap()
        .on_store(&store)
        .execute()
        .unwrap()
    else {
        panic!("CONSTRUCT")
    };
    assert_eq!(
        triples.next().unwrap().unwrap().object,
        Term::Literal(value)
    );
}
