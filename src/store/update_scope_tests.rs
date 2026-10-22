use oxigraph::model::{BlankNode, GraphName, NamedNode, Quad};
use oxigraph::sparql::SparqlEvaluator;
use oxigraph::store::Store;

fn node(value: &str) -> NamedNode {
    NamedNode::new(format!("urn:{value}")).unwrap()
}

#[test]
fn scoped_native_update_uses_exact_read_and_write_graphs() {
    for graph in [
        GraphName::from(node("selected")),
        BlankNode::new("scope").unwrap().into(),
    ] {
        let store = Store::new().unwrap();
        let source = Quad::new(node("s"), node("p"), node("o"), graph.clone());
        let other = Quad::new(node("other"), node("p"), node("o"), node("other"));
        store.insert(&source).unwrap();
        store.insert(&other).unwrap();
        let mut transaction = store.start_transaction().unwrap();
        SparqlEvaluator::new()
            .parse_update("DELETE {?s <urn:p> ?o} INSERT {?s <urn:q> ?o} WHERE {?s <urn:p> ?o}")
            .unwrap()
            .with_default_graph(graph.clone())
            .on_transaction(&mut transaction)
            .execute()
            .unwrap();
        assert!(!transaction.contains(&source).unwrap());
        assert!(
            transaction
                .contains(&Quad::new(node("s"), node("q"), node("o"), graph.clone()))
                .unwrap()
        );
        assert!(transaction.contains(&other).unwrap());
        transaction.commit().unwrap();
        SparqlEvaluator::new()
            .parse_update("CLEAR DEFAULT")
            .unwrap()
            .with_default_graph(graph)
            .on_store(&store)
            .execute()
            .unwrap();
        assert_eq!(store.len().unwrap(), 1);
        assert!(store.contains(&other).unwrap());
    }
}

#[test]
fn explicit_update_dataset_and_templates_override_implicit_scope() {
    let store = Store::new().unwrap();
    let source = Quad::new(node("s"), node("p"), node("o"), node("explicit"));
    store.insert(&source).unwrap();
    for operation in [
        "WITH <urn:explicit> INSERT {?s <urn:q> ?o} WHERE {?s <urn:p> ?o}",
        "INSERT {GRAPH <urn:output> {?s <urn:r> ?o}} USING <urn:explicit> WHERE {?s <urn:p> ?o}",
    ] {
        SparqlEvaluator::new()
            .parse_update(operation)
            .unwrap()
            .with_default_graph(node("implicit").into())
            .on_store(&store)
            .execute()
            .unwrap();
    }
    assert!(
        store
            .contains(&Quad::new(
                node("s"),
                node("q"),
                node("o"),
                node("explicit")
            ))
            .unwrap()
    );
    assert!(
        store
            .contains(&Quad::new(node("s"), node("r"), node("o"), node("output")))
            .unwrap()
    );
    assert_eq!(store.graph_len(&node("implicit")).unwrap(), 0);
}
