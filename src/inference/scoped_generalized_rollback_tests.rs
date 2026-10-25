//! Failure after pending derivation precedes cleanup and rolls back prior writes.
use std::collections::HashSet;

use oxigraph::model::*;
use oxigraph::store::Store;

use super::super::{Graph, Pending, apply_rules, shared::CompiledRule, traversal};
use super::ScopedGraph;

fn iri(value: &str) -> NamedNode {
    NamedNode::new(value).unwrap()
}

#[test]
fn pending_list_error_keeps_generalized_destination_and_rolls_back_prior_writes() {
    let source: GraphName = iri("urn:source").into();
    let destination: GraphName = BlankNode::new_unchecked("urn:source").into();
    let store = Store::new().unwrap();
    let existing = GeneralizedQuad::new(
        iri("urn:existing"),
        BlankNode::new("kept-predicate").unwrap(),
        iri("urn:value"),
        destination.clone(),
    );
    let mut transaction = store.start_transaction().unwrap();
    transaction.insert_generalized(&existing);
    for (s, p, o) in [
        (
            "urn:C",
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
            "http://www.w3.org/2002/07/owl#Class",
        ),
        (
            "urn:chain",
            "http://www.w3.org/2002/07/owl#propertyChainAxiom",
            "urn:list",
        ),
        (
            "urn:list",
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#first",
            "urn:p",
        ),
        (
            "urn:list",
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest",
            "urn:list",
        ),
    ] {
        transaction.insert_generalized(&GeneralizedQuad::new(
            iri(s),
            iri(p),
            iri(o),
            source.clone(),
        ));
    }
    transaction.commit().unwrap();
    let before = store
        .generalized_quads_for_pattern(None, None, None, None)
        .collect::<Result<HashSet<_>, _>>()
        .unwrap();
    {
        let mut transaction = store.start_transaction().unwrap();
        transaction.insert_generalized(&GeneralizedQuad::new(
            iri("urn:prior"),
            iri("urn:p"),
            iri("urn:pending"),
            source.clone(),
        ));
        let data = ScopedGraph {
            transaction: &mut transaction,
            source,
            destination,
        };
        let mut pending = Pending::default();
        apply_rules(
            &data,
            &CompiledRule::for_mode("owlrl").unwrap(),
            &mut pending,
            true,
        )
        .unwrap();
        assert!(
            !pending.is_empty(),
            "error must follow actual pending derivation"
        );
        let error = traversal::chains(
            &data,
            &mut pending,
            &crate::inference::diagnostics::rdf_list::ListWitnesses::new(4300),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("List contains a recursive rdf:rest reference")
        );
        for quad in &pending {
            assert!(
                !data.contains(quad.into()).unwrap(),
                "pending output became visible"
            );
        }
        // No postprocess or publication follows an error. Drop the real owner
        // transaction so its earlier staged insertion is undone as well.
    }
    let after = store
        .generalized_quads_for_pattern(None, None, None, None)
        .collect::<Result<HashSet<_>, _>>()
        .unwrap();
    assert_eq!(after, before);
    assert!(after.contains(&existing));
}
