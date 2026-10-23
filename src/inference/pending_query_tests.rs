//! Existing CONSTRUCT heads must not hash the unrelated missing-fact buffer.

use std::cell::Cell;
use std::collections::{HashSet, hash_map::RandomState};
use std::hash::BuildHasher;
use std::rc::Rc;

use oxigraph::model::{Dataset, GraphName, NamedNode, Quad};
use oxigraph::sparql::SparqlEvaluator;

use super::{Pending, apply_queries};

#[derive(Default)]
struct CountingBuildHasher {
    calls: Rc<Cell<usize>>,
    inner: RandomState,
}

impl BuildHasher for CountingBuildHasher {
    type Hasher = <RandomState as BuildHasher>::Hasher;

    fn build_hasher(&self) -> Self::Hasher {
        self.calls.set(self.calls.get() + 1);
        self.inner.build_hasher()
    }
}

fn quad(subject: &str) -> Quad {
    Quad::new(
        NamedNode::new(subject).unwrap(),
        NamedNode::new("urn:p").unwrap(),
        NamedNode::new("urn:o").unwrap(),
        GraphName::DefaultGraph,
    )
}

#[test]
fn existing_construct_head_does_not_hash_pending_missing_facts() {
    let calls = Rc::new(Cell::new(0));
    let mut pending = Pending(HashSet::with_hasher(CountingBuildHasher {
        calls: Rc::clone(&calls),
        ..Default::default()
    }));
    // A nonempty unrelated pending set prevents HashSet's empty-table shortcut.
    pending.0.insert(quad("urn:unrelated"));
    calls.set(0);
    let existing = quad("urn:existing");
    let mut data = Dataset::from_iter([existing]);
    let transaction = data.insert_transaction();
    let query = SparqlEvaluator::new()
        .parse_query("CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }")
        .unwrap();
    apply_queries(&transaction, &[query], &mut pending).unwrap();
    assert_eq!(pending.0.len(), 1);
    assert_eq!(calls.get(), 0, "source-existing query head hashed pending");
}

#[test]
fn production_pending_keeps_the_original_hashset_layout() {
    assert_eq!(
        std::mem::size_of::<Pending>(),
        std::mem::size_of::<HashSet<Quad>>()
    );
}
