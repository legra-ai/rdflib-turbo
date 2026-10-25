use super::*;
use crate::model::{Literal, NamedNode, Quad};

#[test]
fn cardinality_snapshot_len_does_not_visit_physical_history() {
    let storage = MemoryStorage::new();
    let quad = |i| {
        Quad::new(
            NamedNode::new_unchecked(format!("urn:s{i}")),
            NamedNode::new_unchecked("urn:p"),
            Literal::new_simple_literal("v"),
            GraphNameRef::DefaultGraph,
        )
    };
    let mut tx = storage.start_transaction();
    for i in 0..1000 {
        tx.insert(quad(i).as_ref());
    }
    tx.commit();
    let before = storage.snapshot();
    let mut tx = storage.start_transaction();
    for i in 1..1000 {
        tx.remove(quad(i).as_ref());
    }
    tx.commit();
    let after = storage.snapshot();
    assert_eq!(before.len(), 1000);
    assert!(!before.is_empty());
    assert_eq!(after.len(), 1);
    assert!(!after.is_empty());
    assert_eq!(before.visited_nodes.load(Ordering::Relaxed), 0);
    assert_eq!(after.visited_nodes.load(Ordering::Relaxed), 0);
    assert_eq!(storage.content.quad_set.len(), 1000);
}

#[test]
fn cardinality_snapshot_pair_matches_its_visible_rows_during_publication() {
    let storage = MemoryStorage::new();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            for i in 0..100 {
                let mut tx = storage.start_transaction();
                tx.insert(
                    Quad::new(
                        NamedNode::new_unchecked(format!("urn:s{i}")),
                        NamedNode::new_unchecked("urn:p"),
                        Literal::from(i),
                        GraphNameRef::DefaultGraph,
                    )
                    .as_ref(),
                );
                tx.commit();
            }
        });
        for _ in 0..200 {
            let reader = storage.snapshot();
            assert_eq!(
                reader.len(),
                reader.quads_for_pattern(None, None, None, None).count()
            );
            assert_eq!(reader.len(), reader.snapshot_id);
        }
    });
}
