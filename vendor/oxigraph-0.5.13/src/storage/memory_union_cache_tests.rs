//! Isolated actual-native union and exact-term cache attribution, never a query replacement.
use std::collections::hash_map::Entry;
use std::hash::Hash;

use crate::model::{GeneralizedQuad, GraphName, Literal, NamedNode};
use crate::sparql::{PreparedSparqlQuery, SparqlEvaluator};
use crate::store::Store;
use rustc_hash::{FxHashMap, FxHashSet};

use super::super::EncodedTerm;
use super::{RequestedHeap, measure_requested_heap};

type Key = (EncodedTerm, EncodedTerm, EncodedTerm);
#[derive(Default)]
struct Compact<T> {
    terms: FxHashMap<T, usize>,
    triples: FxHashSet<[usize; 3]>,
}

impl<T: Eq + Hash> Compact<T> {
    fn intern(&mut self, term: T) -> usize {
        let next = self.terms.len();
        match self.terms.entry(term) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                entry.insert(next);
                next
            }
        }
    }

    fn insert(&mut self, (s, p, o): (T, T, T)) -> bool {
        let ids = [self.intern(s), self.intern(p), self.intern(o)];
        self.triples.insert(ids)
    }

    fn contains(&self, (s, p, o): &(T, T, T)) -> bool {
        match (self.terms.get(s), self.terms.get(p), self.terms.get(o)) {
            (Some(s), Some(p), Some(o)) => self.triples.contains(&[*s, *p, *o]),
            _ => false,
        }
    }
}

fn fixture(rows: usize, bytes: usize, distinct: bool, contexts: usize) -> Store {
    assert!(rows <= 70_000 && (rows <= 256 || bytes <= 64));
    assert!((1..=3).contains(&contexts));
    let store = Store::new().unwrap();
    let word = "x".repeat(bytes);
    for index in 0..rows {
        let s = NamedNode::new_unchecked(format!("urn:s:{index}"));
        let p = NamedNode::new_unchecked(if distinct {
            format!("urn:p:{index}")
        } else {
            "urn:p".to_owned()
        });
        let o: crate::model::Term = if distinct {
            NamedNode::new_unchecked(format!("urn:o:{index}:{word}")).into()
        } else {
            Literal::new_simple_literal(&word).into()
        };
        for context in 0..contexts {
            let graph = match context {
                0 => GraphName::DefaultGraph,
                1 => NamedNode::new_unchecked("urn:scope").into(),
                2 => crate::model::BlankNode::new_unchecked("scope").into(),
                _ => unreachable!(),
            };
            store
                .insert_generalized(&GeneralizedQuad::new(
                    s.clone(),
                    p.clone(),
                    o.clone(),
                    graph,
                ))
                .unwrap();
        }
    }
    store
}

fn query(union: bool) -> PreparedSparqlQuery {
    let mut query = SparqlEvaluator::new().parse_query(
        "CONSTRUCT { ?p <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/1999/02/22-rdf-syntax-ns#Property> } WHERE { ?s ?p ?o }"
    ).unwrap();
    if union {
        query.dataset_mut().set_default_graph_as_union();
    }
    query
}

fn native(store: &Store, union: bool, prefix: Option<usize>) -> (usize, RequestedHeap) {
    let query = query(union);
    let transaction = store.start_transaction().unwrap();
    // Warm the existing plan cache before accounting, without retaining its result.
    drop(
        query
            .clone()
            .on_transaction(&transaction)
            .execute_leased_inference_construct()
            .unwrap(),
    );
    let mut count = 0;
    let allocation = measure_requested_heap(|| {
        let heads = query
            .clone()
            .on_transaction(&transaction)
            .execute_leased_inference_construct()
            .unwrap();
        for head in heads.take(prefix.unwrap_or(usize::MAX)) {
            std::hint::black_box(head.unwrap());
            count += 1;
        }
    });
    (count, allocation)
}

fn metrics(label: &str, allocation: &RequestedHeap) {
    println!(
        "UNION_ALLOCATION {{\"arm\":{label:?},\"live_before\":{},\"live_after\":{},\"peak\":{},\"allocated\":{},\"calls\":{}}}",
        allocation.live_before,
        allocation.live_after,
        allocation.peak,
        allocation.allocated,
        allocation.calls
    );
}

#[test]
#[ignore = "isolated requested heap process with explicit bounded probe parameters"]
fn profile_native_union_and_exact_cache_allocations() {
    let number = |name: &str| std::env::var(name).unwrap().parse::<usize>().unwrap();
    let rows = number("UNION_ROWS");
    let bytes = number("UNION_BYTES");
    let contexts = number("UNION_CONTEXTS");
    let arm = std::env::var("UNION_ARM").unwrap();
    let distinct = match std::env::var("UNION_DISTINCT").unwrap().as_str() {
        "0" => false,
        "1" => true,
        _ => panic!("invalid distinct-term selector"),
    };
    let store = fixture(rows, bytes, distinct, contexts);
    let reader = store.storage().snapshot();
    assert_eq!(reader.len().unwrap(), rows * contexts);
    match arm.as_str() {
        "native-union" | "native-default" | "native-prefix" | "native-drop" => {
            assert!(
                !distinct,
                "native output cardinality is deliberately constant"
            );
            let prefix = match arm.as_str() {
                "native-prefix" => Some(1),
                "native-drop" => Some(0),
                _ => None,
            };
            let (count, allocation) = native(&store, arm != "native-default", prefix);
            assert_eq!(count, usize::from(rows != 0 && prefix != Some(0)));
            assert_eq!(allocation.live_after, allocation.live_before);
            metrics(&arm, &allocation);
        }
        "wide" => {
            let mut cache = FxHashSet::<Key>::default();
            let allocation = measure_requested_heap(|| {
                for row in reader.quads_for_pattern(None, None, None, None) {
                    let row = row.unwrap();
                    cache.insert((row.subject, row.predicate, row.object));
                }
                std::hint::black_box(&cache);
            });
            assert_eq!(cache.len(), rows);
            let capacity = cache.capacity();
            let release = measure_requested_heap(|| drop(cache));
            assert_eq!(release.live_after, allocation.live_before);
            metrics(&arm, &allocation);
            metrics("wide-release", &release);
            println!(
                "UNION_CAPACITY {{\"triples\":{rows},\"triple_capacity\":{capacity},\"distinct_terms\":null,\"dictionary_capacity\":0}}"
            );
        }
        "compact" => {
            let mut cache = Compact {
                terms: FxHashMap::default(),
                triples: FxHashSet::default(),
            };
            let allocation = measure_requested_heap(|| {
                for row in reader.quads_for_pattern(None, None, None, None) {
                    let row = row.unwrap();
                    cache.insert((row.subject, row.predicate, row.object));
                }
                std::hint::black_box(&cache);
            });
            assert_eq!(cache.triples.len(), rows);
            let (term_count, term_capacity, triple_capacity) = (
                cache.terms.len(),
                cache.terms.capacity(),
                cache.triples.capacity(),
            );
            let release = measure_requested_heap(|| drop(cache));
            assert_eq!(release.live_after, allocation.live_before);
            metrics(&arm, &allocation);
            metrics("compact-release", &release);
            println!(
                "UNION_CAPACITY {{\"triples\":{rows},\"triple_capacity\":{triple_capacity},\"distinct_terms\":{term_count},\"dictionary_capacity\":{term_capacity}}}"
            );
        }
        _ => panic!("invalid cache attribution arm"),
    }
    assert_eq!(store.len().unwrap(), rows * contexts);
    semantics::semantic_replay(&store, rows);
    println!(
        "UNION_CASE {{\"rows\":{rows},\"contexts\":{contexts},\"payload_bytes\":{bytes},\"all_distinct_terms\":{distinct},\"physical_rows\":{}}}",
        rows * contexts
    );
}

#[test]
#[ignore = "exact isolated requested allocator regression process"]
fn native_union_query_cache_does_not_exceed_sixteen_mebibytes() {
    let store = fixture(68_118, 32, false, 2);
    let (count, allocation) = native(&store, true, None);
    assert_eq!(count, 1);
    metrics("native-union-budget", &allocation);
    assert!(
        allocation.peak - allocation.live_before <= 16 * 1024 * 1024,
        "constant-output native union read cache exceeded16MiB: {allocation:?}"
    );
}

#[test]
fn exact_cache_prototype_matches_every_full_native_term_and_scope_duplicate() {
    let store = fixture(37, 64, true, 3);
    let reader = store.storage().snapshot();
    let mut wide = FxHashSet::default();
    let mut compact = Compact {
        terms: FxHashMap::default(),
        triples: FxHashSet::default(),
    };
    for row in reader.quads_for_pattern(None, None, None, None) {
        let row = row.unwrap();
        let key = (row.subject, row.predicate, row.object);
        assert_eq!(wide.insert(key.clone()), compact.insert(key));
    }
    assert_eq!(
        (wide.len(), compact.triples.len(), compact.terms.len()),
        (37, 37, 111)
    );
    for key in &wide {
        assert!(compact.contains(key));
    }
    assert_eq!(reader.len().unwrap(), 111);
}

#[test]
fn query_local_ids_never_become_cross_owner_term_identity() {
    let a = EncodedTerm::from(NamedNode::new_unchecked("urn:a").as_ref());
    let b = EncodedTerm::from(NamedNode::new_unchecked("urn:b").as_ref());
    let mut left = Compact {
        terms: FxHashMap::default(),
        triples: FxHashSet::default(),
    };
    let mut right = Compact {
        terms: FxHashMap::default(),
        triples: FxHashSet::default(),
    };
    left.insert((a.clone(), a.clone(), a.clone()));
    right.insert((b.clone(), b.clone(), b.clone()));
    assert_eq!(left.triples, right.triples);
    assert!(!left.contains(&(b.clone(), b.clone(), b)));
    assert!(!right.contains(&(a.clone(), a.clone(), a)));
}

#[path = "memory_union_semantic_tests.rs"]
mod semantics;
