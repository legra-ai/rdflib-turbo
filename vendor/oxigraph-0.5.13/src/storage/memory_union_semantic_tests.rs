//! Full canonical relation, scope, cancellation, snapshot and collision controls.
use super::{Compact, EncodedTerm, fixture};
use crate::model::{GeneralizedQuad, GraphName, GraphNameRef, Literal, NamedNode, TermRef};
use crate::sparql::{
    CancellationToken, PreparedSparqlQuery, QueryEvaluationError, SparqlEvaluator,
};
use crate::store::Store;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};

type TermProof = (&'static str, String, Option<String>, Option<String>, bool);
type QuadProof = (TermProof, TermProof, TermProof, Option<TermProof>);

const FULL_QUERY: &str = "CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }";

fn term_proof(term: TermRef<'_>) -> TermProof {
    match term {
        TermRef::NamedNode(node) => ("iri", node.as_str().to_owned(), None, None, false),
        TermRef::BlankNode(node) => ("blank", node.as_str().to_owned(), None, None, false),
        TermRef::Literal(value) => (
            "literal",
            value.value().to_owned(),
            Some(value.datatype().as_str().to_owned()),
            value.language().map(str::to_owned),
            value.has_explicit_datatype(),
        ),
        #[cfg(feature = "rdf-12")]
        TermRef::Triple(_) => panic!("this bounded source fixture contains no quoted terms"),
    }
}

fn physical(store: &Store) -> Vec<QuadProof> {
    // bounded: at most210,000 physical rows in the explicitly capped source probe.
    let mut rows = store
        .generalized_quads_for_pattern(None, None, None, None)
        .map(|row| {
            let row = row.unwrap();
            let row = row.as_ref();
            (
                term_proof(row.subject),
                term_proof(row.predicate),
                term_proof(row.object),
                match row.graph_name {
                    GraphNameRef::DefaultGraph => None,
                    GraphNameRef::NamedNode(node) => Some(term_proof(node.into())),
                    GraphNameRef::BlankNode(node) => Some(term_proof(node.into())),
                },
            )
        })
        .collect::<Vec<_>>();
    rows.sort();
    rows
}

fn full_query() -> PreparedSparqlQuery {
    let mut query = SparqlEvaluator::new().parse_query(FULL_QUERY).unwrap();
    query.dataset_mut().set_default_graph_as_union();
    query
}

pub(super) fn semantic_replay(store: &Store, expected_rows: usize) {
    let before = physical(store);
    // bounded: the complete logical relation of the capped fixture, with no term filtering.
    let expected = before
        .iter()
        .map(|(s, p, o, _)| (s.clone(), p.clone(), o.clone()))
        .collect::<BTreeSet<_>>();
    // bounded: same complete logical relation, evaluated through actual native DatasetView.
    let actual = full_query()
        .on_store(store)
        .execute_inference_construct()
        .unwrap()
        .map(|head| {
            let head = head.unwrap();
            let head = head.as_ref();
            (
                term_proof(head.subject),
                term_proof(head.predicate),
                term_proof(head.object),
            )
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(expected.len(), expected_rows);
    assert_eq!(actual, expected);
    let after = physical(store);
    assert_eq!(before, after);
    println!(
        "UNION_SEMANTICS {}",
        serde_json::to_string(&(before, after, expected, actual)).unwrap()
    );
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CollisionTerm(EncodedTerm);
impl Hash for CollisionTerm {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u8(0);
    }
}

pub(super) fn canonical_role_values() -> [crate::model::Term; 7] {
    [
        NamedNode::new_unchecked("urn:same").into(),
        crate::model::BlankNode::new_unchecked("urn:same").into(),
        Literal::new_simple_literal("urn:same").into(),
        Literal::new_typed_literal(
            "urn:same",
            NamedNode::new_unchecked("http://www.w3.org/2001/XMLSchema#string"),
        )
        .into(),
        Literal::new_typed_literal("urn:same", NamedNode::new_unchecked("urn:datatype")).into(),
        Literal::new_language_tagged_literal("urn:same", "en")
            .unwrap()
            .into(),
        Literal::new_language_tagged_literal("urn:same", "fr")
            .unwrap()
            .into(),
    ]
}

#[test]
fn compact_cache_checks_full_term_identity_even_when_all_term_hashes_collide() {
    let values = canonical_role_values();
    let store = Store::new().unwrap();
    let mut cache = Compact {
        terms: FxHashMap::default(),
        triples: FxHashSet::default(),
    };
    for value in &values {
        let encoded = CollisionTerm(EncodedTerm::from(value.as_ref()));
        assert!(cache.insert((encoded.clone(), encoded.clone(), encoded)));
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
    assert_eq!((cache.terms.len(), cache.triples.len()), (7, 7));
    for (i, left) in values.iter().enumerate() {
        for (j, right) in values.iter().enumerate() {
            let l = CollisionTerm(EncodedTerm::from(left.as_ref()));
            let r = CollisionTerm(EncodedTerm::from(right.as_ref()));
            assert_eq!(cache.contains(&(l.clone(), r, l)), i == j);
        }
    }
    semantic_replay(&store, 7);
}

#[test]
fn native_union_prefix_cancellation_snapshot_and_rollback_preserve_complete_physical_facts() {
    let store = fixture(19, 64, false, 3);
    let before = physical(&store);
    let token = CancellationToken::new();
    let mut query = SparqlEvaluator::new()
        .with_cancellation_token(token.clone())
        .parse_query(FULL_QUERY)
        .unwrap();
    query.dataset_mut().set_default_graph_as_union();
    let mut heads = query
        .on_store(&store)
        .execute_inference_construct()
        .unwrap();
    heads.next().unwrap().unwrap();
    token.cancel();
    let error = heads.next().unwrap().unwrap_err();
    assert!(matches!(error, QueryEvaluationError::Cancelled));
    println!(
        "UNION_CANCELLED {}",
        serde_json::to_string(&error.to_string()).unwrap()
    );
    drop(heads);
    assert_eq!(physical(&store), before);
    let mut snapshot = full_query()
        .on_store(&store)
        .execute_leased_inference_construct()
        .unwrap();
    let leased = snapshot.next().unwrap().unwrap();
    let first = leased.as_ref();
    let first = (
        term_proof(first.subject),
        term_proof(first.predicate),
        term_proof(first.object),
    );
    let addition = GeneralizedQuad::new(
        NamedNode::new_unchecked("urn:late"),
        NamedNode::new_unchecked("urn:p"),
        Literal::new_simple_literal("late"),
        GraphName::DefaultGraph,
    );
    {
        let mut transaction = store.start_transaction().unwrap();
        transaction.insert_generalized(&addition);
        // bounded: the nineteen original triples and one rollback-only addition.
        let closure = full_query()
            .on_transaction(&transaction)
            .execute_inference_construct()
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(closure.len(), 20);
    }
    assert_eq!(physical(&store), before);
    store.insert_generalized(&addition).unwrap();
    // bounded: all nineteen heads from the earlier immutable snapshot, including prefix.
    let mut old = snapshot
        .map(|head| {
            let head = head.unwrap();
            let head = head.as_ref();
            (
                term_proof(head.subject),
                term_proof(head.predicate),
                term_proof(head.object),
            )
        })
        .collect::<BTreeSet<_>>();
    old.insert(first.clone());
    assert_eq!(old.len(), 19);
    let expected = before
        .iter()
        .map(|(s, p, o, _)| (s.clone(), p.clone(), o.clone()))
        .collect::<BTreeSet<_>>();
    assert_eq!(old, expected);
    let after = physical(&store);
    assert_eq!(after.len(), before.len() + 1);
    drop(store);
    let still_alive = leased.as_ref();
    assert_eq!(
        (
            term_proof(still_alive.subject),
            term_proof(still_alive.predicate),
            term_proof(still_alive.object)
        ),
        first
    );
    println!(
        "UNION_LIFETIME {}",
        serde_json::to_string(&(before, after, expected, old, first)).unwrap()
    );
}
