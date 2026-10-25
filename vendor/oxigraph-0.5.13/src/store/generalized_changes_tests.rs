use super::*;
use crate::model::{BlankNode, GeneralizedQuad, GraphName, Literal, NamedNode};
use crate::store::LexicalGraphName;

#[test]
fn committed_memory_changes_own_full_terms_after_later_write_and_store_drop() {
    let store = Store::new().unwrap();
    let removed = GeneralizedQuad::new(
        Literal::new_simple_literal("old"),
        BlankNode::new("predicate").unwrap(),
        Literal::new_simple_literal("value"),
        BlankNode::new("scope").unwrap(),
    );
    let added = GeneralizedQuad::new(
        Literal::new_simple_literal("long new subject value"),
        BlankNode::new("predicate").unwrap(),
        Literal::new_simple_literal("long new object value"),
        NamedNode::new("urn:scope").unwrap(),
    );
    store.insert_generalized(&removed).unwrap();
    let mut transaction = store.start_transaction().unwrap();
    transaction.begin_memory_changes().unwrap();
    transaction.remove_generalized(&removed);
    transaction.insert_generalized(&added);
    let changes = transaction.commit_with_changes().unwrap();
    store.remove_generalized(&added).unwrap();
    drop(store);
    let (additions, removals) = changes.into_parts();
    assert_eq!(
        additions.collect::<Result<Vec<_>, _>>().unwrap(),
        vec![added]
    );
    assert_eq!(
        removals.collect::<Result<Vec<_>, _>>().unwrap(),
        vec![removed]
    );
}

#[test]
fn memory_change_capture_rejects_prior_writes_and_missing_begin_without_commit() {
    let store = Store::new().unwrap();
    let mut transaction = store.start_transaction().unwrap();
    let quad = GeneralizedQuad::new(
        NamedNode::new("urn:s").unwrap(),
        NamedNode::new("urn:p").unwrap(),
        Literal::new_simple_literal("v"),
        GraphNameRef::DefaultGraph,
    );
    transaction.insert_generalized(&quad);
    assert!(transaction.begin_memory_changes().is_err());
    assert!(transaction.commit_with_changes().is_err());
    assert!(store.is_empty().unwrap());
}

#[test]
fn normal_commit_cannot_discard_active_capture_or_publish_its_writes() {
    let store = Store::new().unwrap();
    let mut transaction = store.start_transaction().unwrap();
    transaction.begin_memory_changes().unwrap();
    transaction.insert_generalized(&GeneralizedQuad::new(
        NamedNode::new("urn:s").unwrap(),
        NamedNode::new("urn:p").unwrap(),
        Literal::new_simple_literal("value"),
        GraphNameRef::DefaultGraph,
    ));
    assert!(transaction.commit().is_err());
    assert!(store.is_empty().unwrap());
}

#[test]
fn committed_quad_lexical_projection_preserves_all_graph_name_forms() {
    // bounded: every existing graph-name encoding, including both full-width ID extremes.
    let graphs: [GraphName; 6] = [
        GraphName::DefaultGraph,
        NamedNode::new("urn:graph").unwrap().into(),
        BlankNode::new("short").unwrap().into(),
        BlankNode::new("long-graph-identifier".repeat(8))
            .unwrap()
            .into(),
        BlankNode::new_from_unique_id(0).into(),
        BlankNode::new_from_unique_id(u128::MAX).into(),
    ];
    let store = Store::new().unwrap();
    let mut transaction = store.start_transaction().unwrap();
    transaction.begin_memory_changes().unwrap();
    for graph in &graphs {
        transaction.insert_generalized(&GeneralizedQuad::new(
            NamedNode::new("urn:s").unwrap(),
            NamedNode::new("urn:p").unwrap(),
            Literal::new_simple_literal("value"),
            graph.clone(),
        ));
    }
    let (added, removed) = transaction.commit_with_changes().unwrap().into_parts();
    assert_eq!(removed.count(), 0);
    let actual: Vec<_> = added
        .project_quad_lexical()
        .map(|row| match row.unwrap().graph_name {
            LexicalGraphName::DefaultGraph => (0, String::new()),
            LexicalGraphName::NamedNode(text) => (1, text.as_str().to_owned()),
            LexicalGraphName::BlankNode(text) => (2, text.as_str().to_owned()),
        })
        .collect();
    assert_eq!(actual.len(), graphs.len());
    // This API promises kind + lexical label: new("0") is not the model's
    // explicit numeric-zero constructor. Opaque wire identity has separate tests.
    let expected: Vec<_> = graphs
        .iter()
        .map(|graph| match graph {
            GraphName::DefaultGraph => (0, String::new()),
            GraphName::NamedNode(node) => (1, node.as_str().to_owned()),
            GraphName::BlankNode(node) => (2, node.as_str().to_owned()),
        })
        .collect();
    assert!(
        expected.iter().all(|graph| actual.contains(graph)),
        "expected={expected:?}, actual={actual:?}"
    );
}

#[cfg(all(feature = "rocksdb", not(target_family = "wasm")))]
#[test]
fn persistent_capture_refuses_before_writes_and_keeps_reopened_data() {
    let directory = tempfile::TempDir::new().unwrap();
    let quad = GeneralizedQuad::new(
        NamedNode::new("urn:s").unwrap(),
        NamedNode::new("urn:p").unwrap(),
        Literal::new_simple_literal("v"),
        GraphNameRef::DefaultGraph,
    );
    let store = Store::open(directory.path()).unwrap();
    store.insert_generalized(&quad).unwrap();
    let mut transaction = store.start_transaction().unwrap();
    assert!(transaction.begin_memory_changes().is_err());
    assert!(transaction.commit_with_changes().is_err());
    assert!(store.contains_generalized(&quad).unwrap());
    assert_eq!(store.len().unwrap(), 1);
    drop(store);
    let reopened = Store::open(directory.path()).unwrap();
    assert!(reopened.contains_generalized(&quad).unwrap());
    assert_eq!(reopened.len().unwrap(), 1);
}

#[test]
fn prepared_lexical_changes_stay_unpublished_and_reject_incomplete_or_wrong_owner_acceptance() {
    let store = Store::new().unwrap();
    let original = GeneralizedQuad::new(
        Literal::new_simple_literal("old subject with dictionary words"),
        BlankNode::new("predicate").unwrap(),
        NamedNode::new("urn:object").unwrap(),
        GraphName::DefaultGraph,
    );
    let added = GeneralizedQuad::new(
        Literal::new_simple_literal("new subject with dictionary words"),
        BlankNode::new("predicate").unwrap(),
        NamedNode::new("urn:object").unwrap(),
        BlankNode::new("scope").unwrap(),
    );
    store.insert_generalized(&original).unwrap();
    let mut transaction = store.start_transaction().unwrap();
    transaction.begin_memory_changes().unwrap();
    transaction.remove_generalized(&original);
    transaction.insert_generalized(&added);
    let mut prepared = transaction.prepare_memory_changes().unwrap();
    assert!(store.contains_generalized(&original).unwrap());
    assert!(!store.contains_generalized(&added).unwrap());
    let other = Store::new().unwrap();
    let other_transaction = other.start_transaction().unwrap();
    assert!(
        other_transaction
            .visit_prepared_changes(&mut prepared, true, |_| panic!("foreign rows decoded"))
            .is_err()
    );
    drop(other_transaction);
    let mut rows = Vec::new();
    assert!(
        transaction
            .visit_prepared_changes(&mut prepared, true, |row| {
                rows.push(row);
                false
            })
            .unwrap()
    );
    assert_eq!(rows.len(), 1);
    assert!(transaction.commit_prepared_changes(prepared).is_err());
    assert!(store.contains_generalized(&original).unwrap());
    assert!(!store.contains_generalized(&added).unwrap());
    let mut transaction = store.start_transaction().unwrap();
    transaction.begin_memory_changes().unwrap();
    transaction.remove_generalized(&original);
    transaction.insert_generalized(&added);
    let mut prepared = transaction.prepare_memory_changes().unwrap();
    for added in [true, false] {
        assert!(
            transaction
                .visit_prepared_changes(&mut prepared, added, |row| {
                    rows.push(row);
                    true
                })
                .unwrap()
        );
    }
    transaction.commit_prepared_changes(prepared).unwrap();
    assert!(!store.contains_generalized(&original).unwrap());
    assert!(store.contains_generalized(&added).unwrap());
    drop(store);
    assert_eq!(rows.len(), 3);
    assert!(
        matches!(&rows[0].subject, crate::store::LexicalTerm::Literal { value, .. } if value.as_str() == "new subject with dictionary words")
    );
}
