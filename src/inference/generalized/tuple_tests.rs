use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::*;

use super::{Generalized, Tuples};
use crate::graph::{RDF, iri};
use crate::inference::{OWL, Pending, traversal};

fn named(value: &str) -> Term {
    NamedNode::new(format!("urn:{value}")).unwrap().into()
}

#[test]
fn chain_heads_stay_pending_until_the_common_round_boundary() {
    let source = r#"
        @prefix owl: <http://www.w3.org/2002/07/owl#> .
        <urn:r> owl:propertyChainAxiom (<urn:p> <urn:q>) .
    "#;
    // bounded: one two-property schema list and a single asserted data edge.
    let mut data: Dataset = RdfParser::from_format(RdfFormat::Turtle)
        .for_slice(source.as_bytes())
        .map(|row| row.unwrap())
        .collect();
    let literal: Term = Literal::new_simple_literal("value").into();
    data.insert(&Quad::new(
        NamedNode::new("urn:a").unwrap(),
        NamedNode::new("urn:p").unwrap(),
        literal.clone(),
        GraphName::DefaultGraph,
    ));
    let mut extra = Generalized::default();
    extra.insert([literal.clone(), named("q"), named("a")]);
    let transaction = data.insert_transaction();
    let view = Tuples {
        data: &transaction,
        extra: &extra,
    };
    let mut ordinary = Pending::default();
    let mut pending = Generalized::default();
    traversal::chains(&view, &mut ordinary, &mut pending).unwrap();
    let head = [named("a"), named("r"), named("a")];
    assert!(!view.contains(&head).unwrap());
    assert!(ordinary.0.contains(&Quad::new(
        NamedNode::new("urn:a").unwrap(),
        NamedNode::new("urn:r").unwrap(),
        named("a"),
        GraphName::DefaultGraph
    )));
    assert!(pending.rows.is_empty());
}

#[test]
fn generalized_key_joins_keep_exact_numeric_lexical_identity() {
    let mut data = Dataset::new();
    let list = BlankNode::new("list").unwrap();
    let integer = NamedNode::new("http://www.w3.org/2001/XMLSchema#integer").unwrap();
    let left: Term = Literal::new_typed_literal("01", integer.clone()).into();
    let right: Term = Literal::new_typed_literal("1", integer).into();
    data.insert(&Quad::new(
        NamedNode::new("urn:C").unwrap(),
        iri(OWL, "hasKey"),
        list.clone(),
        GraphName::DefaultGraph,
    ));
    data.insert(&Quad::new(
        list.clone(),
        iri(RDF, "first"),
        named("key"),
        GraphName::DefaultGraph,
    ));
    data.insert(&Quad::new(
        list,
        iri(RDF, "rest"),
        iri(RDF, "nil"),
        GraphName::DefaultGraph,
    ));
    data.insert(&Quad::new(
        NamedNode::new("urn:b").unwrap(),
        iri(RDF, "type"),
        named("C"),
        GraphName::DefaultGraph,
    ));
    data.insert(&Quad::new(
        NamedNode::new("urn:b").unwrap(),
        NamedNode::new("urn:key").unwrap(),
        right,
        GraphName::DefaultGraph,
    ));
    let subject: Term = Literal::new_simple_literal("instance").into();
    let mut extra = Generalized::default();
    extra.insert([subject.clone(), iri(RDF, "type").into(), named("C")]);
    extra.insert([subject.clone(), named("key"), left.clone()]);
    let mut transaction = data.insert_transaction();
    let mut ordinary = Pending::default();
    let mut pending = Generalized::default();
    traversal::keys(
        &Tuples {
            data: &transaction,
            extra: &extra,
        },
        &mut ordinary,
        &mut pending,
    )
    .unwrap();
    assert!(ordinary.0.is_empty());
    assert!(pending.rows.is_empty());
    transaction.insert(&Quad::new(
        NamedNode::new("urn:b").unwrap(),
        NamedNode::new("urn:key").unwrap(),
        left,
        GraphName::DefaultGraph,
    ));
    traversal::keys(
        &Tuples {
            data: &transaction,
            extra: &extra,
        },
        &mut ordinary,
        &mut pending,
    )
    .unwrap();
    assert!(pending.contains(&[subject, iri(OWL, "sameAs").into(), named("b")]));
}
