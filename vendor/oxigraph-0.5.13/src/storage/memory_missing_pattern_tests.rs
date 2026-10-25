use super::{EncodedTerm, MemoryStorage, Ordering};
use crate::model::{GraphNameRef, NamedNodeRef, QuadRef};

#[test]
fn missing_pattern_stops_probing_indexes_after_first_absent_component() {
    let storage = MemoryStorage::new();
    let iri = NamedNodeRef::new_unchecked("urn:existing");
    let graph = NamedNodeRef::new_unchecked("urn:graph");
    let mut transaction = storage.start_transaction();
    transaction.insert(QuadRef::new(iri, iri, iri, graph));
    transaction.commit();
    let present = EncodedTerm::from(iri);
    let absent = EncodedTerm::from(NamedNodeRef::new_unchecked("urn:absent"));
    let graph = EncodedTerm::from(GraphNameRef::NamedNode(graph));
    for missing in 0..4 {
        let reader = storage.snapshot();
        let mut pattern = [&present, &present, &present, &graph];
        pattern[missing] = &absent;
        assert_eq!(
            reader
                .quads_for_pattern(
                    Some(pattern[0]),
                    Some(pattern[1]),
                    Some(pattern[2]),
                    Some(pattern[3]),
                )
                .count(),
            0
        );
        assert_eq!(
            reader.index_probes.load(Ordering::Relaxed),
            missing + 1,
            "remaining indexes were probed after an absent component at {missing}"
        );
    }
}

#[test]
fn missing_pattern_reads_follow_commits_rollback_and_retained_snapshots() {
    let storage = MemoryStorage::new();
    let iri = NamedNodeRef::new_unchecked("urn:existing");
    let graph = NamedNodeRef::new_unchecked("urn:graph");
    let quad = QuadRef::new(iri, iri, iri, graph);
    let encoded = EncodedTerm::from(iri);
    let graph = EncodedTerm::from(GraphNameRef::NamedNode(graph));
    let before = storage.snapshot();
    assert_eq!(
        before
            .quads_for_pattern(Some(&encoded), None, None, None)
            .count(),
        0
    );
    let mut transaction = storage.start_transaction();
    transaction.insert(quad);
    transaction.commit();
    let present = storage.snapshot();
    for mask in 0..16 {
        let pattern = [Some(&encoded), Some(&encoded), Some(&encoded), Some(&graph)];
        let pattern = std::array::from_fn::<_, 4, _>(|i| {
            if mask & (1 << i) == 0 {
                None
            } else {
                pattern[i]
            }
        });
        assert_eq!(
            present
                .quads_for_pattern(pattern[0], pattern[1], pattern[2], pattern[3])
                .count(),
            1
        );
        assert_eq!(
            before
                .quads_for_pattern(pattern[0], pattern[1], pattern[2], pattern[3])
                .count(),
            0
        );
    }
    let mut transaction = storage.start_transaction();
    transaction.remove(quad);
    transaction.commit();
    assert_eq!(
        present
            .quads_for_pattern(Some(&encoded), None, None, Some(&graph))
            .count(),
        1
    );
    assert_eq!(
        storage
            .snapshot()
            .quads_for_pattern(Some(&encoded), None, None, Some(&graph))
            .count(),
        0
    );
    let mut transaction = storage.start_transaction();
    transaction.insert(quad);
    drop(transaction);
    assert_eq!(
        storage
            .snapshot()
            .quads_for_pattern(Some(&encoded), None, None, Some(&graph))
            .count(),
        0
    );
}
