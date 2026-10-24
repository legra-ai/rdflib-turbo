use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::*;

use crate::inference::{InferenceRequest, expand};

fn closure(document: &str) -> Dataset {
    let document = format!(
        r#"
        @prefix owl: <http://www.w3.org/2002/07/owl#> .
        @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
        @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
        {document}
    "#
    );
    // bounded: small fixtures with a finite closure over their input terms.
    let mut data: Dataset = RdfParser::from_format(RdfFormat::Turtle)
        .for_slice(document.as_bytes())
        .map(|quad| quad.unwrap())
        .collect();
    expand(&mut data, "owlrl", InferenceRequest::default()).unwrap();
    data
}

#[test]
fn property_chain_crosses_a_literal_intermediate() {
    let data = closure(
        r#"
        <urn:a> <urn:p> "value" .
        <urn:p> owl:inverseOf <urn:q> .
        <urn:chain> owl:propertyChainAxiom (<urn:p> <urn:q>) .
    "#,
    );
    assert!(data.contains(&Quad::new(
        NamedNode::new("urn:a").unwrap(),
        NamedNode::new("urn:chain").unwrap(),
        NamedNode::new("urn:a").unwrap(),
        GraphName::DefaultGraph,
    )));
}

#[test]
fn intersection_recognizes_a_literal_instance() {
    let data = closure(
        r#"
        <urn:a> <urn:p> "value" .
        <urn:p> rdfs:range <urn:A>, <urn:B> .
        <urn:I> owl:intersectionOf (<urn:A> <urn:B>) .
        rdf:type owl:inverseOf <urn:typed> .
    "#,
    );
    assert!(data.contains(&Quad::new(
        NamedNode::new("urn:I").unwrap(),
        NamedNode::new("urn:typed").unwrap(),
        Literal::new_simple_literal("value"),
        GraphName::DefaultGraph,
    )));
}

#[test]
fn key_recognizes_a_literal_instance() {
    let data = closure(
        r#"
        <urn:a> <urn:p> "value" .
        <urn:p> rdfs:range <urn:C> .
        <urn:v> <urn:back> "value" .
        <urn:back> owl:inverseOf <urn:key> .
        <urn:b> a <urn:C> ; <urn:key> <urn:v> .
        <urn:C> owl:hasKey (<urn:key>) .
    "#,
    );
    assert!(data.contains(&Quad::new(
        NamedNode::new("urn:a").unwrap(),
        NamedNode::new("urn:p").unwrap(),
        NamedNode::new("urn:b").unwrap(),
        GraphName::DefaultGraph,
    )));
}

#[test]
fn max_cardinality_retains_literal_equality() {
    let data = closure(
        r#"
        <urn:R> owl:onProperty <urn:p> ; owl:maxCardinality 1 .
        <urn:x> a <urn:R> ; <urn:p> "left", "right" .
        <urn:y> <urn:q> "left" .
    "#,
    );
    assert!(data.contains(&Quad::new(
        NamedNode::new("urn:y").unwrap(),
        NamedNode::new("urn:q").unwrap(),
        Literal::new_simple_literal("right"),
        GraphName::DefaultGraph,
    )));
}
