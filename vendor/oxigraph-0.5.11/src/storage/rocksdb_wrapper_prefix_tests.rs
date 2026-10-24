//! Exclusive prefix limits must not admit neighboring graph or term identities.
use super::{Db, DbOptions};
use crate::model::*;
use crate::store::Store;
use std::collections::HashSet;

#[test]
fn prefix_scan_carry_excludes_adjacent_keys_and_retains_unbounded_extremes() {
    let directory = tempfile::tempdir().unwrap();
    let db = Db::open_read_write(directory.path(), vec![], DbOptions::default()).unwrap();
    let column = db.column_family("default").unwrap();
    // bounded: fixed raw keys exercise carry, exact prefixes and both no-upper cases.
    let mut keys = vec![vec![], vec![0], vec![1, 2], vec![1, 2, 0xff], vec![1, 3, 0]];
    for width in [8, 16] {
        let mut maximum = vec![8];
        maximum.extend(std::iter::repeat_n(0xff, width));
        keys.push(maximum.clone());
        maximum.push(0);
        keys.push(maximum);
    }
    keys.extend([vec![9, 0], vec![9, 1], vec![0xff], vec![0xff, 0xff]]);
    for key in &keys {
        db.insert(&column, key, b"value").unwrap();
    }
    let mut prefixes = keys.clone();
    prefixes.extend([vec![1], vec![1, 2, 0xff], vec![9], vec![0xff; 16]]);
    for prefix in prefixes {
        let reader = db.snapshot();
        let mut iter = reader.scan_prefix(&column, &prefix);
        let mut actual = Vec::new();
        while let Some(key) = iter.key() {
            actual.push(key.to_vec());
            iter.next();
        }
        iter.status().unwrap();
        let mut expected: Vec<_> = keys
            .iter()
            .filter(|key| key.starts_with(&prefix))
            .cloned()
            .collect();
        expected.sort();
        assert_eq!(actual, expected, "prefix={prefix:?}");
    }
}

fn iri(value: &str) -> NamedNode {
    NamedNode::new(value).unwrap()
}

fn fixture() -> Vec<Quad> {
    // bounded: nine synthetic scopes and two terms per scope, independent of data size.
    let graphs = [
        GraphName::DefaultGraph,
        iri("urn:graph").into(),
        BlankNode::new_unchecked("urn:graph").into(),
        BlankNode::new_from_unique_id(u128::from(u64::MAX)).into(),
        BlankNode::new_from_unique_id(u128::from(u64::MAX) + 1).into(),
        BlankNode::new_from_unique_id(u128::MAX).into(),
        BlankNode::new_unchecked("").into(),
        BlankNode::new("abcdefghijklmno").unwrap().into(),
        BlankNode::new("abcdefghijklmnop").unwrap().into(),
    ];
    let mut rows = Vec::new();
    for graph in graphs {
        for (subject, object) in [
            (
                BlankNode::new_from_unique_id(u128::MAX),
                Term::from(BlankNode::new_from_unique_id(u128::MAX)),
            ),
            (
                BlankNode::new("next").unwrap(),
                Literal::new_simple_literal("value").into(),
            ),
        ] {
            rows.push(Quad::new(
                subject,
                iri("urn:predicate"),
                object,
                graph.clone(),
            ));
        }
    }
    rows
}

fn verify(expected: &[Quad], mut query: impl FnMut(&Quad, u8) -> HashSet<Quad>) {
    for probe in expected {
        // All16 bound masks select the six named and three default key orders.
        for mask in 0..16 {
            let expected: HashSet<_> = expected
                .iter()
                .filter(|row| {
                    (mask & 1 == 0 || row.subject == probe.subject)
                        && (mask & 2 == 0 || row.predicate == probe.predicate)
                        && (mask & 4 == 0 || row.object == probe.object)
                        && (mask & 8 == 0 || row.graph_name == probe.graph_name)
                })
                .cloned()
                .collect();
            assert_eq!(query(probe, mask), expected, "mask={mask}, probe={probe:?}");
        }
    }
}

fn verify_store(store: &Store, expected: &[Quad]) {
    verify(expected, |probe, mask| {
        store
            .quads_for_pattern(
                (mask & 1 != 0).then(|| probe.subject.as_ref()),
                (mask & 2 != 0).then(|| probe.predicate.as_ref()),
                (mask & 4 != 0).then(|| probe.object.as_ref()),
                (mask & 8 != 0).then(|| probe.graph_name.as_ref()),
            )
            .map(Result::unwrap)
            .collect()
    });
}

#[test]
fn maximum_encoded_prefix_scopes_match_memory_through_rollback_snapshot_and_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let memory = Store::new().unwrap();
    let persistent = Store::open(directory.path()).unwrap();
    let rows = fixture();
    for store in [&memory, &persistent] {
        for row in &rows {
            store.insert(row).unwrap();
        }
        verify_store(store, &rows);
        let snapshot = store.read_snapshot();
        {
            let mut transaction = store.start_transaction().unwrap();
            transaction.remove(&rows[0]);
            let extra = Quad::new(
                iri("urn:extra"),
                iri("urn:predicate"),
                Literal::new_simple_literal("pending"),
                BlankNode::new_from_unique_id(u128::MAX),
            );
            transaction.insert(&extra);
            let mut pending = rows.clone();
            pending.remove(0);
            pending.push(extra);
            verify(&pending, |probe, mask| {
                transaction
                    .quads_for_pattern(
                        (mask & 1 != 0).then(|| probe.subject.as_ref()),
                        (mask & 2 != 0).then(|| probe.predicate.as_ref()),
                        (mask & 4 != 0).then(|| probe.object.as_ref()),
                        (mask & 8 != 0).then(|| probe.graph_name.as_ref()),
                    )
                    .map(Result::unwrap)
                    .collect()
            });
            verify(&rows, |probe, mask| {
                snapshot
                    .quads_for_pattern(
                        (mask & 1 != 0).then(|| probe.subject.as_ref()),
                        (mask & 2 != 0).then(|| probe.predicate.as_ref()),
                        (mask & 4 != 0).then(|| probe.object.as_ref()),
                        (mask & 8 != 0).then(|| probe.graph_name.as_ref()),
                    )
                    .map(Result::unwrap)
                    .collect()
            });
            // Drop aborts the pending insert and removal without copying the base.
        }
        verify_store(store, &rows);
    }
    persistent.flush().unwrap();
    drop(persistent);
    let reopened = Store::open(directory.path()).unwrap();
    verify_store(&reopened, &rows);
}
