use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::*;

use super::Generalized;
use crate::inference::{InferenceRequest, expand};

#[test]
fn literal_subject_intermediates_reenter_ordinary_rdf() {
    let document = r#"
        @prefix owl: <http://www.w3.org/2002/07/owl#> .
        @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
        <urn:a> <urn:p> "value" .
        <urn:p> owl:inverseOf <urn:q> .
        <urn:q> rdfs:subPropertyOf <urn:r> .
        <urn:r> owl:inverseOf <urn:s> .
    "#;
    // bounded: four input statements and their small reference closure.
    let mut data: Dataset = RdfParser::from_format(RdfFormat::Turtle)
        .for_slice(document.as_bytes())
        .map(|quad| quad.unwrap())
        .collect();
    expand(&mut data, "owlrl", InferenceRequest::default()).unwrap();
    assert!(data.contains(&Quad::new(
        NamedNode::new("urn:a").unwrap(),
        NamedNode::new("urn:s").unwrap(),
        Literal::new_simple_literal("value"),
        GraphName::DefaultGraph,
    )));
}

#[test]
fn generalized_rows_preserve_exact_identity_and_deduplicate() {
    let mut rows = Generalized::default();
    let predicate: Term = BlankNode::new("predicate").unwrap().into();
    let datatype = NamedNode::new("http://www.w3.org/2001/XMLSchema#integer").unwrap();
    for value in ["1", "01", "1"] {
        rows.insert([
            Literal::new_typed_literal(value, datatype.clone()).into(),
            predicate.clone(),
            NamedNode::new("urn:value").unwrap().into(),
        ]);
    }
    assert_eq!(rows.matching([None, None, None]).count(), 2);
    assert_eq!(
        rows.matching([None, Some(predicate.as_ref()), None])
            .count(),
        2
    );
    let lexical: Term = Literal::new_typed_literal("01", datatype).into();
    assert_eq!(
        rows.matching([Some(lexical.as_ref()), None, None]).count(),
        1
    );
}
