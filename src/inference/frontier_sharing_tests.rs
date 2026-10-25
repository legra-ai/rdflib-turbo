//! Actual-route copy-growth regressions, isolated from the attribution controls.
use super::{
    CompiledRule, Graph, HeadDisposition, Pending, TransactionGraph, fixture, semantic_replay,
};
use crate::allocation_tests::{RequestedAllocation, measure_requested};
use oxigraph::model::BlankNode;
use rdf_reasoning::RuleId;

fn measured_arm(heads: usize, bytes: usize, retain: bool) -> RequestedAllocation {
    let store = fixture(heads, bytes);
    let query = CompiledRule::from_catalogue(RuleId::Rdfs2)
        .unwrap()
        .into_query();
    let mut transaction = store.start_transaction().unwrap();
    let graph = TransactionGraph {
        transaction: &mut transaction,
        destination: BlankNode::new_unchecked("urn:scope").into(),
    };
    let ((pending, count), observed) = measure_requested(|| {
        let mut pending = Pending::default();
        let mut count = 0;
        for row in graph.projected_inference_heads(query.clone()).unwrap() {
            let row = std::hint::black_box(row.unwrap());
            count += 1;
            if retain {
                assert_eq!(
                    pending.push_head(&graph, row).unwrap(),
                    HeadDisposition::Accepted
                );
            }
        }
        (pending, count)
    });
    assert_eq!(count, heads);
    assert_eq!(pending.len(), if retain { heads } else { 0 });
    observed
}

#[test]
#[ignore = "isolated allocation regression on the actual canonical encoded producer"]
fn native_construct_producer_shares_repeated_source_lexical_payload() {
    let short = measured_arm(32, 64, false);
    let long = measured_arm(32, 65536, false);
    let growth = long.allocated.saturating_sub(short.allocated);
    println!("PRODUCER_WORD_GROWTH {growth} short={short:?} long={long:?}");
    assert!(
        growth <= 2 * (65536 - 64) + 16384,
        "producer copied repeated source word per head: {growth}"
    );
}

#[test]
#[ignore = "isolated allocation regression on the actual canonical native frontier"]
fn native_pending_shares_repeated_source_lexical_payload() {
    let short = measured_arm(32, 64, true);
    let long = measured_arm(32, 65536, true);
    let short_retained = short.live_after.saturating_sub(short.live_before);
    let long_retained = long.live_after.saturating_sub(long.live_before);
    let growth = long_retained.saturating_sub(short_retained);
    println!("PENDING_WORD_GROWTH {growth} short={short:?} long={long:?}");
    assert!(
        growth <= 2 * (65536 - 64) + 16384,
        "frontier retained repeated source word per head: {growth}"
    );
}

#[test]
fn compiled_domain_heads_preserve_full_terms_contexts_and_rollback() {
    let proof = semantic_replay(8, 64);
    assert_eq!(proof.dropped_heads.len(), 8);
    assert_eq!(proof.dropped_heads, proof.pending_heads);
    assert_eq!(proof.source_before, proof.source_after_rollback);
    assert_eq!(proof.graphs_before, proof.graphs_after_rollback);
}

#[test]
fn native_frontier_outlives_query_and_source_and_deduplicates_owned_heads() {
    // bounded: eight heads exercise all six subject roles and exact named/blank contexts.
    let (pending, expected) = {
        let store = fixture(8, 4096);
        let query = CompiledRule::from_catalogue(RuleId::Rdfs2)
            .unwrap()
            .into_query();
        let mut transaction = store.start_transaction().unwrap();
        let graph = TransactionGraph {
            transaction: &mut transaction,
            destination: BlankNode::new_unchecked("urn:scope").into(),
        };
        let mut pending = Pending::default();
        let mut expected = Vec::new();
        let mut owned_first = Pending::default();
        for head in graph.projected_inference_heads(query).unwrap() {
            let head = head.unwrap();
            let owned = head.as_ref().into_owned();
            expected.push(owned.clone());
            assert_eq!(
                owned_first.push(&graph, owned.clone()).unwrap(),
                HeadDisposition::Accepted
            );
            let super::super::pending::Head::Native(native) = &head else {
                panic!("native route required");
            };
            assert_eq!(
                owned_first.push_native(&graph, native.clone()).unwrap(),
                HeadDisposition::AlreadyPresent
            );
            assert_eq!(
                pending.push_head(&graph, head).unwrap(),
                HeadDisposition::Accepted
            );
            assert_eq!(
                pending.push(&graph, owned).unwrap(),
                HeadDisposition::AlreadyPresent
            );
        }
        assert_eq!(
            pending, owned_first,
            "canonical equality must not depend on frontier representation"
        );
        (pending, expected)
    };
    // The query projection cache, transaction reader and entire source Store are now gone.
    assert_eq!(pending.len(), expected.len());
    for expected in &expected {
        assert!(pending.contains(expected));
    }
    let actual = pending
        .into_heads()
        .map(|head| head.as_ref().into_owned())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(actual, expected.into_iter().collect());
}

#[test]
#[ignore = "isolated allocation regression on consuming native leased head storage"]
fn consumed_native_frontier_releases_storage_before_iteration_finishes() {
    const HEADS: usize = 4096;
    let store = fixture(HEADS, 64);
    let query = CompiledRule::from_catalogue(RuleId::Rdfs2)
        .unwrap()
        .into_query();
    let mut transaction = store.start_transaction().unwrap();
    let graph = TransactionGraph {
        transaction: &mut transaction,
        destination: BlankNode::new_unchecked("urn:scope").into(),
    };
    let mut pending = Pending::default();
    for head in graph.projected_inference_heads(query).unwrap() {
        assert_eq!(
            pending.push_head(&graph, head.unwrap()).unwrap(),
            HeadDisposition::Accepted
        );
    }
    // The producer and its strong projection cache were dropped before this snapshot.
    let mut heads = pending.into_heads();
    let before = crate::allocation_tests::live_requested_bytes();
    for _ in 0..HEADS / 2 {
        drop(heads.next().unwrap());
    }
    let reclaimed = before.saturating_sub(crate::allocation_tests::live_requested_bytes());
    println!(
        "NATIVE_FRONTIER_RETIREMENT reclaimed_bytes={reclaimed} holder_bytes={}",
        std::mem::size_of::<oxigraph::sparql::LeasedInferenceTriple>()
    );
    assert!(
        reclaimed >= HEADS * std::mem::size_of::<oxigraph::sparql::LeasedInferenceTriple>() / 4
    );
    assert_eq!(heads.count(), HEADS / 2);
}
