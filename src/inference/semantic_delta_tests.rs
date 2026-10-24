//! Acceptance retains full-Term removals separately from newly inferred rows.
use super::semantic_delta::InferenceDelta;
use oxigraph::model::*;
use oxrdf::GeneralizedDataset;
use std::mem::size_of;

fn quad(graph: GraphName) -> GeneralizedQuad {
    GeneralizedQuad::new(
        Literal::new_simple_literal("subject"),
        BlankNode::new_unchecked("predicate"),
        Literal::new_simple_literal("object"),
        graph,
    )
}

#[test]
fn semantic_delta_default_has_no_removal_owner_and_preserves_full_term_scope() {
    let mut delta = InferenceDelta::default();
    assert_eq!(
        size_of::<InferenceDelta>(),
        size_of::<GeneralizedDataset>() + size_of::<usize>()
    );
    assert!(!delta.has_removals());
    let named = quad(NamedNode::new_unchecked("urn:scope").into());
    let blank = quad(BlankNode::new_unchecked("urn:scope").into());
    delta.remove_source(named.clone());
    assert!(delta.is_removed(named.as_ref()));
    assert!(!delta.is_removed(blank.as_ref()));
    assert_eq!(delta.removals().count(), 1);
    delta.remove_source(named);
    assert_eq!(delta.removals().count(), 1);
}

#[test]
fn semantic_delta_acceptance_removes_then_adds_and_unwind_restores_original_records() {
    let removed = quad(GraphName::DefaultGraph);
    let added = quad(NamedNode::new_unchecked("urn:destination").into());
    let mut delta = InferenceDelta::default();
    delta.remove_source(removed.clone());
    delta.additions.insert(&added);
    let mut data = GeneralizedDataset::from_iter([removed.clone()]);
    {
        let mut transaction = data.transaction();
        delta.apply_dataset(&mut transaction);
        assert!(!transaction.contains(&removed));
        assert!(transaction.contains(&added));
    }
    assert!(data.contains(&removed));
    assert!(!data.contains(&added));
    let mut transaction = data.transaction();
    delta.apply_dataset(&mut transaction);
    transaction.commit();
    assert!(!data.contains(&removed));
    assert!(data.contains(&added));
}
