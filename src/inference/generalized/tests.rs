use std::collections::HashSet;

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

#[test]
fn equality_connects_named_aliases_through_a_literal() {
    let document = r#"
        @prefix owl: <http://www.w3.org/2002/07/owl#> .
        <urn:a> owl:sameAs "shared" .
        <urn:b> owl:sameAs "shared" .
        <urn:a> <urn:p> <urn:value> .
    "#;
    // bounded: three input statements and their finite tiny closure.
    let mut data: Dataset = RdfParser::from_format(RdfFormat::Turtle)
        .for_slice(document.as_bytes())
        .map(|quad| quad.unwrap())
        .collect();
    expand(&mut data, "owlrl", InferenceRequest::default()).unwrap();
    assert!(data.contains(&Quad::new(
        NamedNode::new("urn:a").unwrap(),
        NamedNode::new("http://www.w3.org/2002/07/owl#sameAs").unwrap(),
        NamedNode::new("urn:b").unwrap(),
        GraphName::DefaultGraph,
    )));
    assert!(data.contains(&Quad::new(
        NamedNode::new("urn:b").unwrap(),
        NamedNode::new("urn:p").unwrap(),
        NamedNode::new("urn:value").unwrap(),
        GraphName::DefaultGraph,
    )));
}

#[test]
fn compact_reflexivity_matches_exact_tuple_patterns() {
    let datatype = NamedNode::new("http://www.w3.org/2001/XMLSchema#integer").unwrap();
    let left: Term = Literal::new_typed_literal("01", datatype.clone()).into();
    let right: Term = Literal::new_typed_literal("1", datatype).into();
    let same: Term = super::SAME_AS.into();
    let predicate: Term = NamedNode::new("urn:p").unwrap().into();
    let blank: Term = BlankNode::new("predicate").unwrap().into();
    let value: Term = NamedNode::new("urn:value").unwrap().into();
    let expected = [
        [left.clone(), same.clone(), left.clone()],
        [right.clone(), same.clone(), right.clone()],
        [left.clone(), predicate.clone(), value.clone()],
        [value.clone(), blank.clone(), left.clone()],
    ];
    let mut current = Generalized::default();
    for row in &expected {
        current.insert(row.clone());
        current.insert(row.clone());
    }
    for subject in [
        None,
        Some(left.as_ref()),
        Some(right.as_ref()),
        Some(value.as_ref()),
    ] {
        for property in [
            None,
            Some(same.as_ref()),
            Some(predicate.as_ref()),
            Some(blank.as_ref()),
        ] {
            for object in [
                None,
                Some(left.as_ref()),
                Some(right.as_ref()),
                Some(value.as_ref()),
            ] {
                let pattern = [subject, property, object];
                // bounded: four distinct tuples, checked against all64 selector combinations.
                let actual: HashSet<_> = current
                    .matching(pattern)
                    .map(|row| row.map(TermRef::into_owned))
                    .collect();
                let selected: HashSet<_> = expected
                    .iter()
                    .filter(|row| {
                        pattern
                            .iter()
                            .zip(*row)
                            .all(|(bound, term)| bound.is_none_or(|bound| bound == term.as_ref()))
                    })
                    .cloned()
                    .collect();
                assert_eq!(actual, selected, "pattern {pattern:?}");
            }
        }
    }
}

#[test]
fn literal_type_records_preserve_exact_identity_in_every_selector_shape() {
    let datatype = NamedNode::new("http://www.w3.org/2001/XMLSchema#integer").unwrap();
    let left: Term = Literal::new_typed_literal("01", datatype.clone()).into();
    let right: Term = Literal::new_typed_literal("1", datatype.clone()).into();
    let rdf_type: Term = crate::graph::iri(crate::graph::RDF, "type").into();
    let class: Term = BlankNode::new("class").unwrap().into();
    let expected = [
        [left.clone(), rdf_type.clone(), datatype.clone().into()],
        [right.clone(), rdf_type.clone(), datatype.clone().into()],
        [left.clone(), rdf_type.clone(), class.clone()],
    ];
    let mut current = Generalized::default();
    for row in &expected {
        current.insert(row.clone());
        current.insert(row.clone());
    }
    assert_eq!(
        current.rows.len(),
        expected.len(),
        "literal typing stores two independent values, not row identifiers and three columns"
    );
    for subject in [
        None,
        Some(left.as_ref()),
        Some(right.as_ref()),
        Some(class.as_ref()),
    ] {
        for predicate in [None, Some(rdf_type.as_ref()), Some(class.as_ref())] {
            for object in [
                None,
                Some(datatype.as_ref().into()),
                Some(class.as_ref()),
                Some(left.as_ref()),
            ] {
                let pattern = [subject, predicate, object];
                // bounded: three exact tuples checked across48 selector combinations.
                let actual: HashSet<_> = current
                    .matching(pattern)
                    .map(|row| row.map(TermRef::into_owned))
                    .collect();
                let selected: HashSet<_> = expected
                    .iter()
                    .filter(|row| {
                        pattern
                            .iter()
                            .zip(*row)
                            .all(|(bound, term)| bound.is_none_or(|bound| bound == term.as_ref()))
                    })
                    .cloned()
                    .collect();
                assert_eq!(actual, selected, "pattern {pattern:?}");
            }
        }
    }
}
