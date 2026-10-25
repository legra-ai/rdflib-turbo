//! Test-only existing-node leases with full canonical SPO identity.
use std::hash::BuildHasherDefault;
use std::hash::{Hash, Hasher};
use std::mem::{align_of, size_of};
use std::sync::Arc;

use crate::model::{GeneralizedQuad, GraphName, Literal, NamedNode};
use crate::storage::{StorageReader, StorageReaderKind};
use crate::store::Store;
use rustc_hash::{FxHashSet, FxHasher};

use super::super::super::{EncodedQuad, QuadListNode};
use super::{fixture, measure_requested_heap, metrics};

#[derive(Clone)]
struct Lease(Arc<QuadListNode>);

trait NodeKey {
    fn node(&self) -> &QuadListNode;
}

impl NodeKey for Lease {
    fn node(&self) -> &QuadListNode {
        &self.0
    }
}

fn same_spo(left: &QuadListNode, right: &QuadListNode) -> bool {
    left.quad.subject == right.quad.subject
        && left.quad.predicate == right.quad.predicate
        && left.quad.object == right.quad.object
}

fn hash_spo<H: Hasher>(node: &QuadListNode, state: &mut H) {
    node.quad.subject.hash(state);
    node.quad.predicate.hash(state);
    node.quad.object.hash(state);
}

impl PartialEq for Lease {
    fn eq(&self, other: &Self) -> bool {
        same_spo(&self.0, &other.0)
    }
}
impl Eq for Lease {}
impl Hash for Lease {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_spo(&self.0, state);
    }
}

#[derive(Clone)]
struct CachedLease {
    hash: u64,
    node: Arc<QuadListNode>,
}

impl NodeKey for CachedLease {
    fn node(&self) -> &QuadListNode {
        &self.node
    }
}

#[derive(Default)]
struct CollisionHasher;
impl Hasher for CollisionHasher {
    fn write(&mut self, _bytes: &[u8]) {}
    fn finish(&self) -> u64 {
        0
    }
}

impl CachedLease {
    fn new(node: Arc<QuadListNode>) -> Self {
        let mut hasher = FxHasher::default();
        hash_spo(&node, &mut hasher);
        Self {
            hash: hasher.finish(),
            node,
        }
    }
}

impl PartialEq for CachedLease {
    fn eq(&self, other: &Self) -> bool {
        same_spo(&self.node, &other.node)
    }
}
impl Eq for CachedLease {}

impl Hash for CachedLease {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}

fn acquire(reader: &StorageReader<'_>, quad: &EncodedQuad) -> Arc<QuadListNode> {
    let memory = match &reader.kind {
        StorageReaderKind::Memory(memory) => memory,
        #[cfg(all(not(target_family = "wasm"), feature = "rocksdb"))]
        StorageReaderKind::RocksDb(_) => panic!("this attribution fixture requires native memory"),
    };
    // Prototype acquisition uses an extra existing-index lookup. A production lazy
    // iterator would transfer its already-checked node; no CPU equivalence is claimed.
    let node = memory.storage.content.quad_set.get(quad).unwrap();
    assert!(memory.is_node_in_range(&node));
    Arc::clone(&node)
}

pub(super) fn profile(reader: &StorageReader<'_>, rows: usize, cached: bool) {
    fn run<K: Eq + Hash + NodeKey>(
        reader: &StorageReader<'_>,
        rows: usize,
        make: impl Fn(Arc<QuadListNode>) -> K,
        arm: &str,
    ) {
        let mut seen = FxHashSet::default();
        let allocation = measure_requested_heap(|| {
            for quad in reader.quads_for_pattern(None, None, None, None) {
                seen.insert(make(acquire(reader, &quad.unwrap())));
            }
            std::hint::black_box(&seen);
        });
        assert_eq!(seen.len(), rows);
        let capacity = seen.capacity();
        let release = measure_requested_heap(|| drop(seen));
        assert_eq!(release.live_after, allocation.live_before);
        metrics(arm, &allocation);
        metrics("lease-release", &release);
        println!(
            "UNION_CAPACITY {{\"triples\":{rows},\"triple_capacity\":{capacity},\"distinct_terms\":null,\"dictionary_capacity\":0,\"key_bytes\":{},\"key_align\":{}}}",
            size_of::<K>(),
            align_of::<K>()
        );
        // Separate unmeasured replay: numerical release has already completed,
        // and no semantic observation owner was retained across that release.
        // bounded: two complete sets over at most70,000 fixed synthetic SPOs.
        let mut complete = FxHashSet::default();
        let mut reference = FxHashSet::default();
        for quad in reader.quads_for_pattern(None, None, None, None) {
            let quad = quad.unwrap();
            reference.insert((
                quad.subject.clone(),
                quad.predicate.clone(),
                quad.object.clone(),
            ));
            complete.insert(make(acquire(reader, &quad)));
        }
        assert_eq!(complete.len(), reference.len());
        for key in &complete {
            let node = key.node();
            assert!(reference.remove(&(
                node.quad.subject.clone(),
                node.quad.predicate.clone(),
                node.quad.object.clone()
            )));
        }
        assert!(reference.is_empty());
        println!(
            "UNION_LEASE_RELATION {{\"canonical\":{},\"whole_relation_equal\":true}}",
            complete.len()
        );
    }
    if cached {
        run(reader, rows, CachedLease::new, "lease-hash");
    } else {
        run(reader, rows, Lease, "lease");
    }
}

#[test]
fn node_lease_keys_ignore_scope_and_never_use_pointer_identity() {
    let store = fixture(1, 64, false, 3);
    let reader = store.storage().snapshot();
    // bounded: exactly three existing physical nodes for the same canonical SPO.
    let nodes = reader
        .quads_for_pattern(None, None, None, None)
        .map(|q| acquire(&reader, &q.unwrap()))
        .collect::<Vec<_>>();
    assert!(!Arc::ptr_eq(&nodes[0], &nodes[1]));
    let mut seen = FxHashSet::default();
    for node in nodes {
        seen.insert(Lease(node));
    }
    assert_eq!(
        seen.len(),
        1,
        "native lease identity must ignore graph, while comparing full SPO"
    );
}

#[test]
fn cached_node_lease_keys_ignore_scope_and_never_use_pointer_identity() {
    let store = fixture(1, 64, false, 3);
    let reader = store.storage().snapshot();
    let mut seen = FxHashSet::default();
    for q in reader.quads_for_pattern(None, None, None, None) {
        seen.insert(CachedLease::new(acquire(&reader, &q.unwrap())));
    }
    assert_eq!(
        seen.len(),
        1,
        "cached hash is only an accelerator for canonical full SPO identity"
    );
}

#[test]
fn cached_node_hash_collisions_still_compare_every_full_spo() {
    let store = Store::new().unwrap();
    for value in super::semantics::canonical_role_values() {
        for graph in [
            GraphName::DefaultGraph,
            NamedNode::new_unchecked("urn:scope").into(),
        ] {
            store
                .insert_generalized(&GeneralizedQuad::new(
                    value.clone(),
                    value.clone(),
                    value.clone(),
                    graph,
                ))
                .unwrap();
        }
    }
    let reader = store.storage().snapshot();
    let mut seen = FxHashSet::default();
    let mut plain =
        std::collections::HashSet::<Lease, BuildHasherDefault<CollisionHasher>>::default();
    for quad in reader.quads_for_pattern(None, None, None, None) {
        let node = acquire(&reader, &quad.unwrap());
        plain.insert(Lease(Arc::clone(&node)));
        // The forced collision is a hash accelerator only; equality retains all terms.
        seen.insert(CachedLease { hash: 0, node });
    }
    assert_eq!(seen.len(), 7);
    assert_eq!(plain.len(), 7);
    super::semantics::semantic_replay(&store, 7);
}

#[test]
fn native_node_cache_leases_do_not_retain_a_second_store_and_retire_every_node() {
    let store = fixture(37, 64, false, 3);
    let reader = store.storage().snapshot();
    let owner = match &reader.kind {
        StorageReaderKind::Memory(memory) => Arc::downgrade(&memory.storage.content),
        #[cfg(all(not(target_family = "wasm"), feature = "rocksdb"))]
        StorageReaderKind::RocksDb(_) => panic!("native memory fixture required"),
    };
    let mut seen = FxHashSet::default();
    for quad in reader.quads_for_pattern(None, None, None, None) {
        seen.insert(Lease(acquire(&reader, &quad.unwrap())));
    }
    // bounded: exactly37 unique native nodes, keeping only weak observation handles.
    let nodes = seen
        .iter()
        .map(|key| Arc::downgrade(&key.0))
        .collect::<Vec<_>>();
    drop(reader);
    drop(store);
    assert!(
        owner.upgrade().is_none(),
        "query keys must not own a copy of the base store"
    );
    assert!(nodes.iter().all(|node| node.upgrade().is_some()));
    drop(seen);
    assert!(nodes.iter().all(|node| node.upgrade().is_none()));
}

#[test]
fn node_key_witnesses_keep_snapshot_visibility_and_stable_identity_after_rollback() {
    let store = fixture(5, 64, false, 3);
    store
        .insert_named_graph(&NamedNode::new_unchecked("urn:empty"))
        .unwrap();
    let old_reader = store.storage().snapshot();
    // bounded: five logical tuples from the fixed fifteen-physical-row witness.
    let expected = old_reader
        .quads_for_pattern(None, None, None, None)
        .map(|q| {
            let q = q.unwrap();
            (q.subject, q.predicate, q.object)
        })
        .collect::<FxHashSet<_>>();
    let removed = store
        .generalized_quads_for_pattern(None, None, None, None)
        .next()
        .unwrap()
        .unwrap();
    store.remove_generalized(&removed).unwrap();
    let added = GeneralizedQuad::new(
        NamedNode::new_unchecked("urn:added"),
        NamedNode::new_unchecked("urn:p"),
        Literal::new_simple_literal("new"),
        GraphName::DefaultGraph,
    );
    store.insert_generalized(&added).unwrap();
    let mut old_keys = FxHashSet::default();
    for q in old_reader.quads_for_pattern(None, None, None, None) {
        old_keys.insert(Lease(acquire(&old_reader, &q.unwrap())));
    }
    // bounded: all five old logical triples, preserving every canonical term field.
    let actual = old_keys
        .iter()
        .map(|k| {
            (
                k.0.quad.subject.clone(),
                k.0.quad.predicate.clone(),
                k.0.quad.object.clone(),
            )
        })
        .collect::<FxHashSet<_>>();
    assert_eq!(actual, expected);
    assert_eq!(old_reader.len().unwrap(), 15);
    let rollback = GeneralizedQuad::new(
        NamedNode::new_unchecked("urn:rollback"),
        NamedNode::new_unchecked("urn:p"),
        Literal::new_simple_literal("rollback"),
        NamedNode::new_unchecked("urn:rollback-graph"),
    );
    let keys = {
        let mut transaction = store.start_transaction().unwrap();
        transaction.insert_generalized(&rollback);
        let reader = transaction.inner().reader();
        let mut keys = FxHashSet::default();
        for q in reader.quads_for_pattern(None, None, None, None) {
            keys.insert(CachedLease::new(acquire(&reader, &q.unwrap())));
        }
        assert_eq!(keys.len(), 7);
        keys
    };
    assert!(!store.contains_generalized(rollback.as_ref()).unwrap());
    assert!(
        !store
            .contains_named_graph(NamedNode::new_unchecked("urn:rollback-graph").as_ref())
            .unwrap()
    );
    assert!(
        store
            .contains_named_graph(NamedNode::new_unchecked("urn:empty").as_ref())
            .unwrap()
    );
    assert_eq!(keys.len(), 7);
    for key in &keys {
        assert!(
            keys.contains(&CachedLease::new(Arc::clone(&key.node))),
            "changing visibility must never mutate the immutable SPO hash/equality fields"
        );
    }
    super::semantics::semantic_replay(&store, 6);
}
