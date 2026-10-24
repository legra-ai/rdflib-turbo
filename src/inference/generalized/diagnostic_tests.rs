use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::*;

use crate::inference::{InferenceRequest, expand};

fn infer(document: &str) -> (Dataset, Vec<String>) {
    let document = format!(
        r#"
        @prefix owl: <http://www.w3.org/2002/07/owl#> .
        @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
        @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
        @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
        {document}
    "#
    );
    // bounded: each hand-written fixture has a finite small closure.
    let mut data: Dataset = RdfParser::from_format(RdfFormat::Turtle)
        .for_slice(document.as_bytes())
        .map(|quad| quad.unwrap())
        .collect();
    let outcome = expand(&mut data, "owlrl", InferenceRequest::default()).unwrap();
    (data, outcome.error_messages)
}

#[test]
fn disjoint_classes_report_a_literal_instance() {
    let (_, errors) = infer(
        r#"
        <urn:a> <urn:p> "value" .
        <urn:p> rdfs:range <urn:A>, <urn:B> .
        <urn:A> owl:disjointWith <urn:B> .
    "#,
    );
    assert!(
        errors.contains(&"Disjoint classes urn:A and urn:B have a common individual value".into())
    );
}

#[test]
fn nothing_reports_a_literal_instance() {
    let (_, errors) = infer(
        r#"
        <urn:a> <urn:p> "value" . <urn:p> rdfs:range owl:Nothing .
    "#,
    );
    assert!(errors.contains(&"value is defined of type 'Nothing'".into()));
}

#[test]
fn asymmetric_property_reports_literal_endpoints() {
    let (_, errors) = infer(
        r#"
        <urn:a> <urn:p> "value" .
        <urn:p> a owl:AsymmetricProperty ; owl:inverseOf <urn:p> .
    "#,
    );
    assert!(
        errors.contains(&"Erroneous usage of asymmetric property urn:p on urn:a and value".into())
    );
}

#[test]
fn negative_assertion_reports_a_literal_subject() {
    let (_, errors) = infer(
        r#"
        <urn:a> <urn:p> "value" . <urn:p> owl:inverseOf <urn:q> .
        <urn:assertion> owl:sourceIndividual "value" ; owl:assertionProperty <urn:q> ; owl:targetIndividual <urn:a> .
    "#,
    );
    assert!(errors.contains(
        &"Negative (object) property assertion violated for: (value, urn:q, urn:a)".into()
    ));
}

#[test]
fn literal_datatype_membership_reenters_ordinary_rdf() {
    let (data, _) = infer(
        r#"
        <urn:a> <urn:p> "1"^^xsd:integer .
        xsd:decimal rdfs:subClassOf <urn:C> .
        rdf:type owl:inverseOf <urn:typed> .
    "#,
    );
    assert!(data.contains(&Quad::new(
        NamedNode::new("urn:C").unwrap(),
        NamedNode::new("urn:typed").unwrap(),
        Literal::new_typed_literal(
            "1",
            NamedNode::new("http://www.w3.org/2001/XMLSchema#integer").unwrap()
        ),
        GraphName::DefaultGraph,
    )));
}
