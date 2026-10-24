//! The inference reader must retain every stored RDF term role and scope.
use std::collections::HashSet;

use oxigraph::model::*;
use oxigraph::store::Store;

use super::super::backend::Graph;
use super::ScopedGraph;

#[test]
fn scoped_inference_reads_literal_subjects_and_non_iri_predicates_without_narrowing() {
    let same = NamedNode::new("urn:scope").unwrap();
    let source: GraphName = same.clone().into();
    let destination: GraphName = BlankNode::new("urn:scope").unwrap().into();
    let terms: [Term; 3] = [
        same.into(),
        BlankNode::new("term").unwrap().into(),
        Literal::new_typed_literal(
            "01",
            NamedNode::new("http://www.w3.org/2001/XMLSchema#integer").unwrap(),
        )
        .into(),
    ];
    let store = Store::new().unwrap();
    let mut transaction = store.start_transaction().unwrap();
    let mut expected = HashSet::new();
    for s in &terms {
        for p in &terms {
            let q = GeneralizedQuad::new(
                s.clone(),
                p.clone(),
                Literal::new_simple_literal("object"),
                source.clone(),
            );
            transaction.insert_generalized(&q);
            expected.insert(q);
        }
    }
    transaction.insert_generalized(&GeneralizedQuad::new(
        Literal::new_simple_literal("destination-only"),
        BlankNode::new("destination-predicate").unwrap(),
        terms[0].clone(),
        destination.clone(),
    ));
    let graph = ScopedGraph {
        transaction: &mut transaction,
        source,
        destination,
    };
    let actual = graph
        .quads(None, None, None)
        .collect::<crate::Result<HashSet<_>>>()
        .unwrap();
    assert_eq!(actual, expected);
}
