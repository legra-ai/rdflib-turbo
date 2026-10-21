use oxigraph::model::{BlankNode, GraphName, NamedNode, Quad};
use oxigraph::store::Store;

const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";

fn iri(value: &str) -> NamedNode {
    NamedNode::new(value).unwrap()
}

fn quad(subject: &str, predicate: &str, object: &str, graph: &GraphName) -> Quad {
    Quad::new(iri(subject), iri(predicate), iri(object), graph.clone())
}

#[test]
fn default_named_and_blank_sources_exclude_outside_schema() {
    // bounded: three graph kinds and three source triples per fixture.
    for source in [
        GraphName::DefaultGraph,
        iri("urn:source").into(),
        BlankNode::new("source").unwrap().into(),
    ] {
        let store = Store::new().unwrap();
        let outside = iri("urn:outside").into();
        let mut transaction = store.start_transaction().unwrap();
        transaction.insert(&quad("urn:x", TYPE, "urn:C", &source));
        transaction.insert(&quad("urn:C", SUBCLASS, "urn:D", &source));
        transaction.insert(&quad("urn:D", SUBCLASS, "urn:Outside", &outside));
        super::expand(
            &mut transaction,
            "rdfs",
            Some(source.clone()),
            source.clone(),
        )
        .unwrap();
        assert!(
            transaction
                .contains(&quad("urn:x", TYPE, "urn:D", &source))
                .unwrap()
        );
        assert!(
            !transaction
                .contains(&quad("urn:x", TYPE, "urn:Outside", &source))
                .unwrap()
        );
    }
}

#[test]
fn separate_destination_terminates_without_becoming_a_rule_source() {
    let store = Store::new().unwrap();
    let source = iri("urn:source").into();
    let destination: GraphName = BlankNode::new("destination").unwrap().into();
    let mut transaction = store.start_transaction().unwrap();
    transaction.insert(&quad("urn:x", TYPE, "urn:C", &source));
    transaction.insert(&quad("urn:C", SUBCLASS, "urn:D", &source));
    transaction.insert(&quad("urn:D", SUBCLASS, "urn:E", &source));
    super::expand(
        &mut transaction,
        "rdfs",
        Some(source.clone()),
        destination.clone(),
    )
    .unwrap();
    assert!(
        transaction
            .contains(&quad("urn:x", TYPE, "urn:D", &destination))
            .unwrap()
    );
    assert!(
        !transaction
            .contains(&quad("urn:x", TYPE, "urn:E", &destination))
            .unwrap()
    );
    assert!(
        !transaction
            .contains(&quad("urn:x", TYPE, "urn:D", &source))
            .unwrap()
    );
}

#[test]
fn union_source_keeps_existing_store_inference_behavior() {
    let store = Store::new().unwrap();
    let schema = iri("urn:schema").into();
    let data = iri("urn:data").into();
    let destination = GraphName::DefaultGraph;
    let mut transaction = store.start_transaction().unwrap();
    transaction.insert(&quad("urn:x", TYPE, "urn:C", &data));
    transaction.insert(&quad("urn:C", SUBCLASS, "urn:D", &schema));
    super::expand(&mut transaction, "rdfs", None, destination.clone()).unwrap();
    assert!(
        transaction
            .contains(&quad("urn:x", TYPE, "urn:D", &destination))
            .unwrap()
    );
    assert!(
        !transaction
            .contains(&quad("urn:x", TYPE, "urn:C", &destination))
            .unwrap()
    );
}
