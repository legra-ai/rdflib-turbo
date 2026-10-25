//! The typed head path must retain the existing measured query-body work.
use json_event_parser::{JsonEvent, SliceJsonParser};
use oxrdf::{
    GeneralizedDataset, GeneralizedQuad, GeneralizedTriple, GraphName, Literal, NamedNode,
};

use crate::{QueryEvaluator, QueryExplanation, QueryResults};

fn work(explanation: &QueryExplanation) -> usize {
    // bounded: one fixed small synthetic query plan, independent of graph size.
    let mut bytes = Vec::new();
    explanation.write_in_json(&mut bytes).unwrap();
    let mut parser = SliceJsonParser::new(&bytes);
    let mut count = false;
    let mut total = 0;
    loop {
        match parser.parse_next().unwrap() {
            JsonEvent::ObjectKey(key) => count = key == "number of results",
            JsonEvent::Number(value) if count => {
                total += value.parse::<usize>().unwrap();
                count = false;
            }
            JsonEvent::Eof => return total,
            _ => count = false,
        }
    }
}

#[test]
fn inference_construct_explanation_retains_positive_strict_body_work() {
    let mut data = GeneralizedDataset::new();
    for value in ["first", "second"] {
        data.insert(&GeneralizedQuad::new(
            NamedNode::new("urn:s").unwrap(),
            NamedNode::new("urn:p").unwrap(),
            Literal::new_simple_literal(value),
            GraphName::DefaultGraph,
        ));
    }
    let query = spargebra::SparqlParser::new()
        .parse_query("CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o VALUES ?duplicate { 1 1 } }")
        .unwrap();
    let evaluator = QueryEvaluator::new().compute_statistics();
    let (strict, strict_explanation) = evaluator.prepare(&query).explain(&data);
    let QueryResults::Graph(strict) = strict.unwrap() else {
        panic!("strict graph");
    };
    let strict = strict
        .map(|row| {
            let row = row.unwrap();
            GeneralizedTriple {
                subject: row.subject.into(),
                predicate: row.predicate.into(),
                object: row.object,
            }
        })
        .collect::<Vec<_>>();
    let (full, full_explanation) = evaluator.prepare(&query).explain_inference_construct(&data);
    let full = full.unwrap().map(Result::unwrap).collect::<Vec<_>>();
    assert_eq!(full, strict);
    let strict_work = work(&strict_explanation);
    assert!(strict_work > 0, "actual body results must be measured");
    assert_eq!(work(&full_explanation), strict_work);
}

#[test]
fn inference_construct_explanation_keeps_empty_construct_distinct_from_wrong_form() {
    let evaluator = QueryEvaluator::new().compute_statistics();
    let data = GeneralizedDataset::new();
    let query = spargebra::SparqlParser::new()
        .parse_query("CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }")
        .unwrap();
    let (rows, _) = evaluator.prepare(&query).explain_inference_construct(&data);
    assert_eq!(rows.unwrap().count(), 0);
    let query = spargebra::SparqlParser::new()
        .parse_query("ASK { ?s ?p ?o }")
        .unwrap();
    let (rows, _) = evaluator.prepare(&query).explain_inference_construct(&data);
    assert!(rows.is_err());
}
