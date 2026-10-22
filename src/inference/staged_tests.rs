//! Preserve the previous materialized-union algorithm as an independent oracle.
use std::time::Instant;

use super::*;

fn fixture() -> Dataset {
    let mut data = Dataset::new();
    let subject = BlankNode::new("subject").unwrap();
    let class = NamedNode::new_unchecked("urn:Child");
    for graph in [
        GraphName::DefaultGraph,
        NamedNode::new_unchecked("urn:source").into(),
        BlankNode::new("source").unwrap().into(),
    ] {
        data.insert(&Quad::new(
            subject.clone(),
            crate::graph::iri(crate::graph::RDF, "type"),
            class.clone(),
            graph.clone(),
        ));
        for value in [
            Literal::new_simple_literal("word"),
            Literal::new_typed_literal("word", crate::graph::iri(crate::graph::XSD, "string")),
        ] {
            data.insert(&Quad::new(
                subject.clone(),
                NamedNode::new_unchecked("urn:p"),
                value,
                graph.clone(),
            ));
        }
    }
    data.insert(&Quad::new(
        class,
        crate::graph::iri(crate::graph::RDFS, "subClassOf"),
        NamedNode::new_unchecked("urn:Parent"),
        NamedNode::new_unchecked("urn:source"),
    ));
    data
}

fn frozen(
    data: &mut Dataset,
    mode: &str,
    destination: GraphName,
    request: InferenceRequest,
) -> InferenceOutcome {
    // bounded: this test's finite synthetic input, retained only as the old oracle.
    let mut union = crate::graph::with_union(data);
    let mut transaction = union.insert_transaction();
    let (prepared, outcome) = expand_configured(&mut transaction, mode, request).unwrap();
    // bounded: at most one publication per literal-valued synthetic input quad.
    let publications: Dataset = if mode == "rdfs" {
        data.iter()
            .filter(|q| q.object.is_literal())
            .map(|q| Quad::new(q.subject, q.predicate, q.object, destination.clone()))
            .filter(|q| !data.contains(q))
            .collect()
    } else {
        Dataset::new()
    };
    let mut output = data.insert_transaction();
    for quad in &publications {
        output.insert(quad);
    }
    for quad in transaction.inserted() {
        output.insert(&Quad::new(
            quad.subject,
            quad.predicate,
            quad.object,
            destination.clone(),
        ));
    }
    prepared
        .insert(|mut quad| {
            quad.graph_name = destination.clone();
            output.insert(&quad);
        })
        .unwrap();
    output.commit();
    transaction.commit();
    outcome
}

#[test]
fn borrowed_union_preserves_all_modes_destinations_and_optional_axioms() {
    for mode in ["rdfs", "owlrl", "both"] {
        for destination in [
            GraphName::DefaultGraph,
            NamedNode::new_unchecked("urn:destination").into(),
            BlankNode::new("destination").unwrap().into(),
        ] {
            for axiomatic_triples in [false, true] {
                for datatype_axioms in [false, true] {
                    let options = InferenceOptions {
                        axiomatic_triples,
                        datatype_axioms,
                    };
                    let input = fixture();
                    let mut expected = input.clone();
                    let old_outcome =
                        frozen(&mut expected, mode, destination.clone(), options.into());
                    let mut actual = input.clone();
                    let (delta, outcome) =
                        expand_delta(&mut actual, mode, Some(destination.clone()), options.into())
                            .unwrap();
                    assert_eq!(
                        actual, expected,
                        "mode={mode}, destination={destination}, axioms={axiomatic_triples}/{datatype_axioms}"
                    );
                    // bounded: differences in the fixed synthetic closure only.
                    let expected_delta: Dataset =
                        expected.iter().filter(|q| !input.contains(*q)).collect();
                    assert_eq!(delta, expected_delta);
                    assert_eq!(outcome.error_messages, old_outcome.error_messages);
                }
            }
        }
    }
}

#[test]
fn failed_borrowed_inference_preserves_source_and_destination() {
    let input = fixture();
    let mut actual = input.clone();
    assert!(
        expand_delta(
            &mut actual,
            "invalid",
            Some(NamedNode::new_unchecked("urn:destination").into()),
            InferenceRequest::default()
        )
        .is_err()
    );
    assert_eq!(actual, input);
}

#[test]
#[ignore = "explicit local timing comparison against the frozen named-union algorithm"]
fn benchmark_borrowed_named_union_against_frozen() {
    let mut input = Dataset::new();
    let class = NamedNode::new_unchecked("urn:Child");
    let graph = NamedNode::new_unchecked("urn:source");
    // bounded: fixed 2,000-subject synthetic benchmark, never production input.
    for index in 0..2000 {
        input.insert(&Quad::new(
            NamedNode::new_unchecked(format!("urn:s:{index}")),
            crate::graph::iri(crate::graph::RDF, "type"),
            class.clone(),
            graph.clone(),
        ));
    }
    input.insert(&Quad::new(
        class,
        crate::graph::iri(crate::graph::RDFS, "subClassOf"),
        NamedNode::new_unchecked("urn:Parent"),
        graph,
    ));
    for mode in ["rdfs", "owlrl"] {
        let mut old = input.clone();
        let start = Instant::now();
        frozen(
            &mut old,
            mode,
            GraphName::DefaultGraph,
            InferenceRequest::default(),
        );
        let old_time = start.elapsed();
        let mut current = input.clone();
        let start = Instant::now();
        expand_into(
            &mut current,
            mode,
            GraphName::DefaultGraph,
            InferenceRequest::default(),
        )
        .unwrap();
        println!(
            "NAMED_UNION mode={mode} frozen={old_time:?} borrowed={:?}",
            start.elapsed()
        );
        assert_eq!(current, old);
    }
}
