//! Public scopes, transaction ownership and reopen preserve every graph domain.
use super::numeric_encoder::{EncodedQuad, EncodedTerm};
use crate::model::*;
use crate::store::Store;
use std::mem::size_of;

fn iri(value: &str) -> NamedNode {
    NamedNode::new(value).unwrap()
}

fn graphs() -> Vec<GraphName> {
    // bounded: the default graph plus every numeric/inline/hashed blank form.
    vec![
        GraphName::DefaultGraph,
        iri("urn:graph").into(),
        // A programmatic blank label may equal an IRI's lexical text; the
        // existing Store accepts the typed value without conflating its kind.
        BlankNode::new_unchecked("urn:graph").into(),
        BlankNode::new_from_unique_id(0).into(),
        BlankNode::new_from_unique_id(u128::MAX).into(),
        BlankNode::new("abcdefghijklmno").unwrap().into(),
        BlankNode::new("abcdefghijklmnop").unwrap().into(),
        BlankNode::new("éééééééa").unwrap().into(),
        BlankNode::new("éééééééé").unwrap().into(),
    ]
}

fn row(index: usize, graph: GraphName) -> Quad {
    Quad::new(
        iri("urn:subject"),
        iri("urn:predicate"),
        Literal::from(i64::try_from(index).unwrap()),
        graph,
    )
}

#[test]
fn compact_encoded_graph_roles_reduce_actual_quad_layout() {
    // Independent of allocator bins: reduce the actual stored quad, not merely
    // round an enlarged node to the same platform-specific allocation bin.
    assert!(size_of::<EncodedQuad>() < 4 * size_of::<EncodedTerm>());
}

#[test]
fn compact_graph_scopes_keep_pending_rollback_and_owned_snapshot_identity() {
    let store = Store::new().unwrap();
    let graphs = graphs();
    {
        let mut transaction = store.start_transaction().unwrap();
        for (index, graph) in graphs.iter().enumerate() {
            transaction.insert(&row(index, graph.clone()));
            assert_eq!(
                transaction
                    .quads_for_pattern(None, None, None, Some(graph.as_ref()))
                    .map(Result::unwrap)
                    .collect::<Vec<_>>(),
                vec![row(index, graph.clone())]
            );
        }
        assert!(store.is_empty().unwrap());
        // Dropping rolls back facts and graph declarations together.
    }
    assert!(store.is_empty().unwrap());
    assert_eq!(store.named_graphs().count(), 0);
    let empty = BlankNode::new("empty-scope").unwrap();
    let before = store.read_snapshot();
    let mut transaction = store.start_transaction().unwrap();
    for (index, graph) in graphs.iter().enumerate() {
        transaction.insert(&row(index, graph.clone()));
    }
    transaction.insert_named_graph(empty.as_ref());
    transaction.commit().unwrap();
    assert_eq!(before.quads_for_pattern(None, None, None, None).count(), 0);
    let pinned = store.read_snapshot();
    assert!(pinned.contains_named_graph(empty.as_ref().into()).unwrap());
    for (index, graph) in graphs.iter().enumerate() {
        assert_eq!(
            pinned
                .quads_for_pattern(None, None, None, Some(graph.as_ref()))
                .map(Result::unwrap)
                .collect::<Vec<_>>(),
            vec![row(index, graph.clone())]
        );
        store.remove(&row(index, graph.clone())).unwrap();
    }
    assert!(store.is_empty().unwrap());
    drop(store);
    assert_eq!(
        pinned.quads_for_pattern(None, None, None, None).count(),
        graphs.len()
    );
}

#[cfg(all(not(target_family = "wasm"), feature = "rocksdb"))]
#[test]
fn compact_graph_scopes_reopen_with_exact_names_and_empty_declarations() {
    let directory = tempfile::tempdir().unwrap();
    let graphs = graphs();
    let empty = BlankNode::new("empty-scope").unwrap();
    let store = Store::open(directory.path()).unwrap();
    let mut transaction = store.start_transaction().unwrap();
    for (index, graph) in graphs.iter().enumerate() {
        transaction.insert(&row(index, graph.clone()));
    }
    transaction.insert_named_graph(empty.as_ref());
    transaction.commit().unwrap();
    let snapshot = store.read_snapshot();
    store.flush().unwrap();
    drop(store);
    assert_eq!(
        snapshot.quads_for_pattern(None, None, None, None).count(),
        graphs.len()
    );
    drop(snapshot);
    let reopened = Store::open(directory.path()).unwrap();
    assert!(reopened.contains_named_graph(empty.as_ref()).unwrap());
    for (index, graph) in graphs.into_iter().enumerate() {
        assert_eq!(
            reopened
                .quads_for_pattern(None, None, None, Some(graph.as_ref()))
                .map(Result::unwrap)
                .collect::<Vec<_>>(),
            vec![row(index, graph)]
        );
    }
    reopened.validate().unwrap();
}
