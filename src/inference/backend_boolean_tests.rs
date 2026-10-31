//! Default and union destination booleans borrow the caller's terms.
use oxigraph::model::*;
use oxigraph::store::Store;

use oxrdf::GeneralizedDataset;

use super::{Graph, TransactionGraph};

#[test]
fn default_dataset_membership_does_not_match_only_named_input() {
    let row = Quad::new(
        NamedNode::new("urn:s").unwrap(),
        NamedNode::new("urn:p").unwrap(),
        Literal::new_simple_literal("value"),
        NamedNode::new("urn:named").unwrap(),
    );
    let mut data = GeneralizedDataset::from_iter([&row]);
    let mut transaction = data.transaction();
    assert!(!Graph::contains(&transaction, (&row).into()).unwrap());
    transaction.insert(QuadRef {
        graph_name: GraphNameRef::DefaultGraph,
        ..row.as_ref()
    });
    assert!(Graph::contains(&transaction, (&row).into()).unwrap());
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
    assert!(data.contains((&row).into()).unwrap());
    assert!(!data.contains_destination((&row).into()).unwrap());
    data.insert(row.clone().into()).unwrap();
    assert!(data.contains_destination((&row).into()).unwrap());
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
    let mut data = GeneralizedDataset::from_iter([&row]);
    let transaction = data.transaction();
    let (present, allocated) = crate::allocation_tests::current_thread::measure_allocated(|| {
        Graph::contains(&transaction, (&row).into()).unwrap()
    });
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
    let generalized = GeneralizedQuad::new(
        Literal::new_simple_literal(&text),
        BlankNode::new(format!("predicate{text}")).unwrap(),
        Literal::new_typed_literal(
            &text,
            NamedNode::new(format!("urn:datatype:{text}")).unwrap(),
        ),
        row.graph_name.clone(),
    );
    store.insert_generalized(&generalized).unwrap();
    let (present, allocated) = crate::allocation_tests::current_thread::measure_allocated(|| {
        store.contains_generalized(&generalized).unwrap()
    });
    assert!(present);
    assert_eq!(
        allocated, 0,
        "Store generalized boolean allocated term payload"
    );
    let mut transaction = store.start_transaction().unwrap();
    let (present, allocated) = crate::allocation_tests::current_thread::measure_allocated(|| {
        transaction.contains_generalized(&generalized).unwrap()
    });
    assert!(present);
    assert_eq!(
        allocated, 0,
        "transaction generalized boolean allocated term payload"
    );
    transaction.insert(&row);
    let data = TransactionGraph {
        transaction: &mut transaction,
        destination: row.graph_name.clone(),
    };
    assert!(data.contains_destination((&row).into()).unwrap());
    let (present, allocated) = crate::allocation_tests::current_thread::measure_allocated(|| {
        data.contains_destination((&row).into()).unwrap()
    });
    assert!(present);
    assert_eq!(
        allocated, 0,
        "transaction destination boolean cloned bound lexical payload"
    );
}

#[test]
fn generalized_point_membership_preserves_roles_graph_kinds_and_transaction_rollback() {
    let named = NamedNode::new("urn:identity").unwrap();
    let blank = BlankNode::new("urn:identity").unwrap();
    let literal = Literal::new_typed_literal(
        "01",
        NamedNode::new("http://www.w3.org/2001/XMLSchema#integer").unwrap(),
    );
    let terms: [Term; 3] = [named.clone().into(), blank.clone().into(), literal.into()];
    let graphs: [GraphName; 3] = [GraphName::DefaultGraph, named.into(), blank.into()];
    for subject in &terms {
        for predicate in &terms {
            for graph in &graphs {
                let store = Store::new().unwrap();
                let row = GeneralizedQuad::new(
                    subject.clone(),
                    predicate.clone(),
                    Literal::new_simple_literal("value"),
                    graph.clone(),
                );
                assert!(!store.contains_generalized(&row).unwrap());
                store.insert_generalized(&row).unwrap();
                assert!(store.contains_generalized(&row).unwrap());
                {
                    let mut tx = store.start_transaction().unwrap();
                    assert!(tx.contains_generalized(&row).unwrap());
                    for other in &graphs {
                        let mut candidate = row.clone();
                        candidate.graph_name = other.clone();
                        assert_eq!(tx.contains_generalized(&candidate).unwrap(), graph == other);
                    }
                    tx.remove_generalized(&row);
                    assert!(!tx.contains_generalized(&row).unwrap());
                    assert!(store.contains_generalized(&row).unwrap());
                    tx.insert_generalized(&row);
                    assert!(tx.contains_generalized(&row).unwrap());
                    tx.remove_generalized(&row);
                }
                assert!(store.contains_generalized(&row).unwrap());
            }
        }
    }
}
