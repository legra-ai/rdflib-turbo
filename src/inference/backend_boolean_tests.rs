//! Default and union destination booleans borrow the caller's terms.
use oxigraph::model::*;
use oxigraph::store::Store;

use super::{Graph, TransactionGraph};

#[test]
fn default_dataset_membership_does_not_match_only_named_input() {
    let row = Quad::new(
        NamedNode::new("urn:s").unwrap(),
        NamedNode::new("urn:p").unwrap(),
        Literal::new_simple_literal("value"),
        NamedNode::new("urn:named").unwrap(),
    );
    let mut data = Dataset::from_iter([&row]);
    let mut transaction = data.insert_transaction();
    assert!(!Graph::contains(&transaction, &row).unwrap());
    transaction.insert(QuadRef {
        graph_name: GraphNameRef::DefaultGraph,
        ..row.as_ref()
    });
    assert!(Graph::contains(&transaction, &row).unwrap());
}

#[test]
fn union_membership_and_destination_membership_remain_distinct() {
    let store = Store::new().unwrap();
    let row = Quad::new(
        NamedNode::new("urn:s").unwrap(),
        NamedNode::new("urn:p").unwrap(),
        Literal::new_simple_literal("value"),
        NamedNode::new("urn:source").unwrap(),
    );
    let mut transaction = store.start_transaction().unwrap();
    transaction.insert(&row);
    let mut data = TransactionGraph {
        transaction: &mut transaction,
        destination: NamedNode::new("urn:destination").unwrap().into(),
    };
    assert!(data.contains(&row).unwrap());
    assert!(!data.contains_destination(&row).unwrap());
    data.insert(row.clone()).unwrap();
    assert!(data.contains_destination(&row).unwrap());
}

#[cfg(feature = "allocation-profiling")]
#[test]
#[ignore = "global allocation counters require isolated execution"]
fn default_dataset_boolean_does_not_clone_bound_lexical_terms() {
    let text = "x".repeat(64 * 1024);
    let row = Quad::new(
        NamedNode::new(format!("urn:s:{text}")).unwrap(),
        NamedNode::new(format!("urn:p:{text}")).unwrap(),
        Literal::new_simple_literal(&text),
        GraphName::DefaultGraph,
    );
    let mut data = Dataset::from_iter([&row]);
    let transaction = data.insert_transaction();
    let (present, allocated) =
        crate::allocation_tests::measure_allocated(|| Graph::contains(&transaction, &row).unwrap());
    assert!(present);
    assert_eq!(
        allocated, 0,
        "default Dataset boolean cloned bound lexical payload"
    );
}

#[cfg(feature = "allocation-profiling")]
#[test]
#[ignore = "global allocation counters require isolated execution"]
fn transaction_destination_boolean_does_not_clone_bound_lexical_terms() {
    let text = "x".repeat(64 * 1024);
    let row = Quad::new(
        NamedNode::new(format!("urn:s:{text}")).unwrap(),
        NamedNode::new(format!("urn:p:{text}")).unwrap(),
        Literal::new_simple_literal(&text),
        NamedNode::new(format!("urn:g:{text}")).unwrap(),
    );
    let store = Store::new().unwrap();
    let mut transaction = store.start_transaction().unwrap();
    transaction.insert(&row);
    let data = TransactionGraph {
        transaction: &mut transaction,
        destination: row.graph_name.clone(),
    };
    assert!(data.contains_destination(&row).unwrap());
    let (present, allocated) =
        crate::allocation_tests::measure_allocated(|| data.contains_destination(&row).unwrap());
    assert!(present);
    assert_eq!(
        allocated, 0,
        "transaction destination boolean cloned bound lexical payload"
    );
}
