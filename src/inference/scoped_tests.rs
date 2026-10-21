use oxigraph::model::{BlankNode, GraphName, Literal, NamedNode, Quad};
use oxigraph::store::Store;

const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";

#[test]
fn rdfs_literal_identity_is_published_even_when_present_only_in_source() {
    // bounded: scoped/union reads and the three inference modes.
    for union in [false, true] {
        for mode in ["rdfs", "owlrl", "both"] {
            let store = Store::new().unwrap();
            let source: GraphName = iri("urn:source").into();
            let destination: GraphName = iri("urn:destination").into();
            let input = Quad::new(
                iri("urn:s"),
                iri("urn:label"),
                Literal::new_simple_literal("label"),
                source.clone(),
            );
            let expected = Quad::new(
                input.subject.clone(),
                input.predicate.clone(),
                input.object.clone(),
                destination.clone(),
            );
            let mut transaction = store.start_transaction().unwrap();
            transaction.insert(&input);
            super::expand(
                &mut transaction,
                mode,
                (!union).then_some(source),
                destination,
            )
            .unwrap();
            assert_eq!(
                transaction.contains(&expected).unwrap(),
                mode == "rdfs",
                "union={union}, mode={mode}"
            );
            assert!(transaction.contains(&input).unwrap());
        }
    }
}

#[test]
fn legacy_dataset_destination_and_delta_include_literal_identity_publication() {
    // bounded: the two legacy Dataset entry points and two source quads.
    for delta in [false, true] {
        let source: GraphName = iri("urn:source").into();
        let destination: GraphName = iri("urn:destination").into();
        let original = Quad::new(
            iri("urn:s"),
            iri("urn:label"),
            Literal::new_simple_literal("label"),
            source.clone(),
        );
        let publication = Quad::new(
            original.subject.clone(),
            original.predicate.clone(),
            original.object.clone(),
            destination.clone(),
        );
        let mut data = oxigraph::model::Dataset::from_iter([
            original.clone(),
            quad("urn:s", TYPE, "urn:C", &source),
        ]);
        if delta {
            let additions =
                super::super::expand_delta(&mut data, "rdfs", Some(destination.clone())).unwrap();
            assert!(additions.contains(&publication));
            assert!(!additions.contains(&original));
            assert!(
                super::super::expand_delta(&mut data, "rdfs", Some(destination.clone()))
                    .unwrap()
                    .iter()
                    .all(|quad| quad != publication.as_ref())
            );
        } else {
            super::super::expand_into(&mut data, "rdfs", destination.clone()).unwrap();
        }
        assert!(data.contains(&original));
        assert!(data.contains(&publication));
        assert!(!data.contains(&quad("urn:s", TYPE, "urn:C", &destination)));
    }
}

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
