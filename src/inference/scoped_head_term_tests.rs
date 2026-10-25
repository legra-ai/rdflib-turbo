//! Explicit inference heads preserve every role and the source/destination boundary.
use std::collections::HashSet;

use oxigraph::model::*;
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;

use super::super::{Graph, HeadDisposition, Pending};
use super::ScopedGraph;

#[test]
fn scoped_inference_heads_keep_literal_subjects_and_apply_pending_predicate_policy() {
    let source: GraphName = NamedNode::new("urn:scope").unwrap().into();
    let destination: GraphName = BlankNode::new("urn:scope").unwrap().into();
    let terms: [Term; 3] = [
        NamedNode::new("urn:term").unwrap().into(),
        BlankNode::new("term").unwrap().into(),
        Literal::new_simple_literal("term").into(),
    ];
    let object: Term = Literal::new_typed_literal(
        "01",
        NamedNode::new("http://www.w3.org/2001/XMLSchema#integer").unwrap(),
    )
    .into();
    let store = Store::new().unwrap();
    let mut input = HashSet::new();
    let mut expected = HashSet::new();
    for subject in &terms {
        for predicate in &terms {
            let quad = GeneralizedQuad::new(
                subject.clone(),
                predicate.clone(),
                object.clone(),
                source.clone(),
            );
            store.insert_generalized(&quad).unwrap();
            input.insert(quad);
            expected.insert(GeneralizedTriple {
                subject: object.clone(),
                predicate: predicate.clone(),
                object: subject.clone(),
            });
        }
    }
    let existing = GeneralizedQuad::new(
        object.clone(),
        terms[2].clone(),
        terms[0].clone(),
        destination.clone(),
    );
    store.insert_generalized(&existing).unwrap();
    let outside = GeneralizedQuad::new(
        NamedNode::new("urn:outside").unwrap(),
        terms[0].clone(),
        object,
        destination.clone(),
    );
    store.insert_generalized(&outside).unwrap();
    let query = SparqlEvaluator::new()
        .parse_query("CONSTRUCT { ?o ?p ?s } WHERE { ?s ?p ?o }")
        .unwrap();
    {
        let mut transaction = store.start_transaction().unwrap();
        let mut graph = ScopedGraph {
            transaction: &mut transaction,
            source: source.clone(),
            destination: destination.clone(),
        };
        let heads = graph
            .inference_heads(query.clone())
            .unwrap()
            .map(|head| head.map_err(crate::Error::from))
            .collect::<crate::Result<HashSet<_>>>()
            .unwrap();
        assert_eq!(heads, expected);
        let QueryResults::Graph(strict) = graph.query(query.clone()).unwrap() else {
            panic!("strict graph");
        };
        assert_eq!(
            strict.count(),
            0,
            "ordinary CONSTRUCT still rejects literal subjects"
        );
        let mut pending = Pending::default();
        let mut dispositions = [0; 3];
        for head in heads {
            match pending
                .push(
                    &graph,
                    GeneralizedQuad::new(
                        head.subject,
                        head.predicate,
                        head.object,
                        GraphName::DefaultGraph,
                    ),
                )
                .unwrap()
            {
                HeadDisposition::Accepted => dispositions[0] += 1,
                HeadDisposition::AlreadyPresent => dispositions[1] += 1,
                HeadDisposition::RefusedLiteralPredicate => dispositions[2] += 1,
            }
        }
        assert_eq!(dispositions, [6, 1, 2]);
        for quad in pending {
            graph.insert(quad).unwrap();
        }
        let source_rows = graph
            .quads(None, None, None)
            .collect::<crate::Result<HashSet<_>>>()
            .unwrap();
        assert_eq!(
            source_rows, input,
            "destination heads never become source rows"
        );
    }
    assert_eq!(
        store.len().unwrap(),
        input.len() + 2,
        "uncommitted heads roll back"
    );
    for quad in input.into_iter().chain([existing, outside]) {
        assert!(store.contains_generalized(&quad).unwrap());
    }
}
