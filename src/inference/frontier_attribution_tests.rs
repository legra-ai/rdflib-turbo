//! Test-only attribution on the canonical encoded inference adapter.
use std::mem::size_of;

use oxigraph::model::{
    BlankNode, GeneralizedQuad, GeneralizedQuadRef, GeneralizedTriple, GraphName, GraphNameRef,
    Literal, NamedNode, Term, TermRef,
    vocab::{rdf, rdfs, xsd},
};
use oxigraph::store::Store;
use rdf_reasoning::RuleId;
use serde::Serialize;

use super::{Graph, HeadDisposition, Pending, backend::TransactionGraph, shared::CompiledRule};
use crate::allocation_tests::{RequestedAllocation, measure_requested};

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
struct TermProof {
    kind: &'static str,
    value: String,
    datatype: Option<String>,
    language: Option<String>,
    explicit_datatype: bool,
}

fn term(value: TermRef<'_>) -> TermProof {
    let (kind, value, datatype, language, explicit_datatype) = match value {
        TermRef::NamedNode(value) => ("iri", value.as_str(), None, None, false),
        TermRef::BlankNode(value) => ("blank", value.as_str(), None, None, false),
        TermRef::Literal(value) => (
            "literal",
            value.value(),
            Some(value.datatype().as_str().to_owned()),
            value.language().map(str::to_owned),
            value.has_explicit_datatype(),
        ),
    };
    TermProof {
        kind,
        value: value.to_owned(),
        datatype,
        language,
        explicit_datatype,
    }
}

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
struct QuadProof {
    subject: TermProof,
    predicate: TermProof,
    object: TermProof,
    graph: Option<TermProof>,
}

fn quad(value: GeneralizedQuadRef<'_>) -> QuadProof {
    QuadProof {
        subject: term(value.subject),
        predicate: term(value.predicate),
        object: term(value.object),
        graph: match value.graph_name {
            GraphNameRef::DefaultGraph => None,
            GraphNameRef::NamedNode(node) => Some(term(node.into())),
            GraphNameRef::BlankNode(node) => Some(term(node.into())),
        },
    }
}

fn class(bytes: usize) -> NamedNode {
    NamedNode::new_unchecked(format!("urn:domain-class:{}", "x".repeat(bytes)))
}

fn subject(index: usize) -> Term {
    let group = index / 6;
    match index % 6 {
        0 => NamedNode::new_unchecked(format!("urn:subject:{group}")).into(),
        1 => BlankNode::new_unchecked(format!("urn:subject:{group}")).into(),
        2 => Literal::new_simple_literal(format!("subject {group}")).into(),
        3 => Literal::new_typed_literal(format!("subject {group}"), xsd::STRING).into(),
        4 => Literal::new_language_tagged_literal(format!("subject {group}"), "en")
            .unwrap()
            .into(),
        _ => Literal::new_language_tagged_literal(format!("subject {group}"), "fr")
            .unwrap()
            .into(),
    }
}

fn scope(index: usize) -> GraphName {
    match index % 3 {
        0 => GraphName::DefaultGraph,
        1 => NamedNode::new_unchecked("urn:scope").into(),
        _ => BlankNode::new_unchecked("urn:scope").into(),
    }
}

fn source_rows(heads: usize, bytes: usize) -> Vec<GeneralizedQuad> {
    // bounded: the probe permits at most128 heads plus one schema/context duplicate.
    let mut rows = vec![GeneralizedQuad::new(
        NamedNode::new_unchecked("urn:p"),
        rdfs::DOMAIN,
        class(bytes),
        NamedNode::new_unchecked("urn:schema"),
    )];
    for index in 0..heads {
        rows.push(GeneralizedQuad::new(
            subject(index),
            NamedNode::new_unchecked("urn:p"),
            Literal::new_simple_literal("value"),
            scope(index),
        ));
    }
    let mut duplicate = rows[1].clone();
    duplicate.graph_name = scope(1);
    rows.push(duplicate);
    rows
}

fn fixture(heads: usize, bytes: usize) -> Store {
    let store = Store::new().unwrap();
    store
        .insert_named_graph(&NamedNode::new_unchecked("urn:empty"))
        .unwrap();
    for row in source_rows(heads, bytes) {
        store.insert_generalized(&row).unwrap();
    }
    store
}

fn source_proof(store: &Store) -> Vec<QuadProof> {
    // bounded: at most130 source rows in this exact probe fixture.
    let mut rows = store
        .generalized_quads_for_pattern(None, None, None, None)
        .map(|row| quad(row.unwrap().as_ref()))
        .collect::<Vec<_>>();
    rows.sort();
    rows
}

fn graphs(store: &Store) -> Vec<TermProof> {
    // bounded: the schema, named/blank scope and declared empty graph.
    let mut graphs = store
        .named_graphs()
        .map(|node| term(node.unwrap().as_ref().into()))
        .collect::<Vec<_>>();
    graphs.sort();
    graphs
}

fn head(value: GeneralizedTriple) -> GeneralizedQuad {
    GeneralizedQuad::new(
        value.subject,
        value.predicate,
        value.object,
        GraphName::DefaultGraph,
    )
}

#[derive(Serialize)]
struct SemanticProof {
    source_before: Vec<QuadProof>,
    source_after_rollback: Vec<QuadProof>,
    dropped_heads: Vec<QuadProof>,
    pending_heads: Vec<QuadProof>,
    published_relation: Vec<QuadProof>,
    graphs_before: Vec<TermProof>,
    graphs_after_rollback: Vec<TermProof>,
}

fn semantic_replay(heads: usize, bytes: usize) -> SemanticProof {
    let store = fixture(heads, bytes);
    let source_before = source_proof(&store);
    let graphs_before = graphs(&store);
    let query = CompiledRule::from_catalogue(RuleId::Rdfs2)
        .unwrap()
        .into_query();
    let destination = BlankNode::new_unchecked("urn:scope");
    let (mut dropped_heads, mut pending_heads, mut published_relation);
    {
        let mut transaction = store.start_transaction().unwrap();
        let mut graph = TransactionGraph {
            transaction: &mut transaction,
            destination: destination.clone().into(),
        };
        // bounded: exactly one missing head per fixed probe subject.
        dropped_heads = graph
            .inference_heads(query.clone())
            .unwrap()
            .map(|row| quad(head(row.unwrap()).as_ref()))
            .collect::<Vec<_>>();
        let mut pending = Pending::default();
        for row in graph.inference_heads(query).unwrap() {
            assert_eq!(
                pending.push(&graph, head(row.unwrap())).unwrap(),
                HeadDisposition::Accepted
            );
        }
        pending_heads = (&pending)
            .into_iter()
            .map(|row| quad(row.as_ref()))
            .collect::<Vec<_>>();
        assert_eq!(pending.len(), heads);
        for row in pending {
            graph.insert(row).unwrap();
        }
        published_relation = graph
            .transaction
            .generalized_quads_for_pattern(None, None, None, None)
            .map(|row| quad(row.unwrap().as_ref()))
            .collect::<Vec<_>>();
        let mut expected = source_rows(heads, bytes)
            .into_iter()
            .map(|row| quad(row.as_ref()))
            .collect::<Vec<_>>();
        expected.extend((0..heads).map(|index| {
            quad(
                GeneralizedQuad::new(subject(index), rdf::TYPE, class(bytes), destination.clone())
                    .as_ref(),
            )
        }));
        published_relation.sort();
        expected.sort();
        assert_eq!(published_relation, expected);
    }
    dropped_heads.sort();
    pending_heads.sort();
    assert_eq!(dropped_heads.len(), heads);
    assert_eq!(dropped_heads, pending_heads);
    let source_after_rollback = source_proof(&store);
    let graphs_after_rollback = graphs(&store);
    assert_eq!(source_before, source_after_rollback);
    assert_eq!(graphs_before, graphs_after_rollback);
    SemanticProof {
        source_before,
        source_after_rollback,
        dropped_heads,
        pending_heads,
        published_relation,
        graphs_before,
        graphs_after_rollback,
    }
}

#[derive(Serialize)]
struct Attribution {
    kind: &'static str,
    route: &'static str,
    rule: &'static str,
    arm: String,
    heads: usize,
    class_iri_bytes: usize,
    source_quads: usize,
    allocations: RequestedAllocation,
    generalized_quad_bytes: usize,
    term_bytes: usize,
    pending_container_bytes: usize,
    semantic_replay: SemanticProof,
}

#[test]
#[ignore = "isolated requested-allocation attribution; explicit fixed fixture and arm required"]
fn canonical_encoded_construct_drop_versus_pending_attribution() {
    let heads = std::env::var("ATTRIBUTION_HEADS")
        .unwrap()
        .parse::<usize>()
        .unwrap();
    let bytes = std::env::var("ATTRIBUTION_LEXICAL_BYTES")
        .unwrap()
        .parse::<usize>()
        .unwrap();
    let arm = std::env::var("ATTRIBUTION_ARM").unwrap();
    assert!([8, 32, 128].contains(&heads));
    assert!([64, 4096, 65536].contains(&bytes));
    assert!(["drop", "pending"].contains(&arm.as_str()));
    let store = fixture(heads, bytes);
    let query = CompiledRule::from_catalogue(RuleId::Rdfs2)
        .unwrap()
        .into_query();
    let mut transaction = store.start_transaction().unwrap();
    let graph = TransactionGraph {
        transaction: &mut transaction,
        destination: BlankNode::new_unchecked("urn:scope").into(),
    };
    let ((pending, count), allocations) = measure_requested(|| {
        let mut pending = Pending::default();
        let mut count = 0;
        for row in graph.inference_heads(query.clone()).unwrap() {
            // Both arms must materialize the same decoded head even when its
            // consumer immediately drops it; do not measure dead-code elision.
            let row = std::hint::black_box(head(row.unwrap()));
            count += 1;
            if arm == "pending" {
                assert_eq!(
                    pending.push(&graph, row).unwrap(),
                    HeadDisposition::Accepted
                );
            } else {
                drop(row);
            }
        }
        (pending, count)
    });
    assert_eq!(count, heads);
    assert_eq!(pending.len(), if arm == "pending" { heads } else { 0 });
    drop(pending);
    drop(transaction);
    // Semantic extraction and its complete raw arrays are outside the measured arm.
    assert_eq!(source_proof(&store), {
        let mut expected = source_rows(heads, bytes)
            .into_iter()
            .map(|row| quad(row.as_ref()))
            .collect::<Vec<_>>();
        expected.sort();
        expected
    });
    let source_quads = store.len().unwrap();
    drop(store);
    let proof = semantic_replay(heads, bytes);
    let report = Attribution {
        kind: "requested-Rust-heap-source-probe",
        route: "canonical-store-TransactionGraph-DatasetView-EncodedTerm",
        rule: "Rdfs2",
        arm,
        heads,
        class_iri_bytes: class(bytes).as_str().len(),
        source_quads,
        allocations,
        generalized_quad_bytes: size_of::<GeneralizedQuad>(),
        term_bytes: size_of::<Term>(),
        pending_container_bytes: size_of::<Pending>(),
        semantic_replay: proof,
    };
    println!(
        "FRONTIER_ATTRIBUTION {}",
        serde_json::to_string(&report).unwrap()
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
