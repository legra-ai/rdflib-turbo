//! Synthetic one-round semantics and deterministic intermediate-work contracts.
use oxrdf::GeneralizedDataset;
use std::cell::Cell;
use std::collections::HashSet;

use oxigraph::model::*;
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults, SparqlEvaluator};

use super::{Graph, OWL_RULES, PREFIXES, Pending, apply_rules};
use crate::Result;

const ORIGINAL: &[(&str, &str)] = &[
    (
        "?s owl:sameAs ?s . ?p owl:sameAs ?p . ?o owl:sameAs ?o",
        "?s ?p ?o",
    ),
    ("?y owl:sameAs ?x", "?x owl:sameAs ?y"),
    ("?x owl:sameAs ?z", "?x owl:sameAs ?y . ?y owl:sameAs ?z"),
    ("?y ?p ?o", "?x owl:sameAs ?y . ?x ?p ?o"),
    ("?s ?q ?o", "?p owl:sameAs ?q . ?s ?p ?o"),
    ("?s ?p ?y", "?x owl:sameAs ?y . ?s ?p ?x"),
];

#[path = "same_as_measurement_tests.rs"]
mod measurement;
use measurement::Measured;

fn iri(value: &str) -> NamedNode {
    NamedNode::new(format!("urn:{value}")).unwrap()
}
fn same() -> NamedNode {
    NamedNode::new("http://www.w3.org/2002/07/owl#sameAs").unwrap()
}
fn queries(rules: &[(&str, &str)]) -> Vec<PreparedSparqlQuery> {
    // bounded: the fixed built-in rule inventory.
    rules
        .iter()
        .map(|(template, pattern)| {
            SparqlEvaluator::new()
                .parse_query(&format!(
                    "{PREFIXES} CONSTRUCT {{ {template} }} WHERE {{ {pattern} }}"
                ))
                .unwrap()
        })
        .collect()
}
fn reference(
    data: &GeneralizedDataset,
    queries: &[PreparedSparqlQuery],
) -> HashSet<GeneralizedQuad> {
    // bounded: exhaustive fixtures have at most a few terms and their one-round consequences.
    let mut result = HashSet::new();
    for query in queries {
        let QueryResults::Graph(triples) =
            query.clone().on_queryable_dataset(data).execute().unwrap()
        else {
            panic!()
        };
        for triple in triples {
            let t = triple.unwrap();
            let quad =
                GeneralizedQuad::new(t.subject, t.predicate, t.object, GraphName::DefaultGraph);
            if !data.contains(&quad) {
                result.insert(quad);
            }
        }
    }
    let view = Measured {
        data,
        work: Cell::new(0),
    };
    let full = super::same_as_reference_tests::equality_rules_one_round(&view).unwrap();
    assert!(
        result.is_subset(&full),
        "strict CONSTRUCT remains an independent control"
    );
    if full
        .iter()
        .all(|q| !q.subject.is_literal() && q.predicate.is_named_node())
    {
        assert_eq!(full, result, "ordinary-domain equality remains exact");
    }
    full
}

#[test]
fn closed_same_as_clique_has_quadratic_round_work() {
    const N: usize = 12;
    let mut data = GeneralizedDataset::new();
    for i in 0..N {
        for j in 0..N {
            data.insert(
                GeneralizedQuad::new(
                    iri(&format!("n{i}")),
                    same(),
                    iri(&format!("n{j}")),
                    GraphName::DefaultGraph,
                )
                .as_ref(),
            );
        }
    }
    let measured = Measured {
        data: &data,
        work: Cell::new(0),
    };
    let mut additions = Pending::default();
    apply_rules(&measured, &queries(OWL_RULES), &mut additions, true).unwrap();
    println!(
        "closed_same_as_round_work={} budget={}",
        measured.work.get(),
        20 * N * N
    );
    assert!(
        measured.work.get() <= 20 * N * N,
        "closed equality round performed {} work units, budget {}",
        measured.work.get(),
        20 * N * N
    );
}

#[test]
fn equality_round_matches_frozen_queries_exhaustively() {
    let original = queries(ORIGINAL);
    let selected: Vec<_> = OWL_RULES
        .iter()
        .copied()
        .filter(|(template, _)| ORIGINAL.iter().any(|(t, _)| t == template))
        .collect();
    let optimized = queries(&selected);
    let a = Term::from(iri("a"));
    let b = Term::from(BlankNode::new("b").unwrap());
    let literal = Term::from(Literal::new_simple_literal("literal"));
    for (subjects, objects, extra) in [
        (
            vec![a.clone(), b.clone()],
            vec![a.clone(), b.clone(), literal.clone()],
            false,
        ),
        (
            vec![a.clone(), Term::from(same()), Term::from(iri("q"))],
            vec![a.clone(), Term::from(same()), Term::from(iri("q"))],
            true,
        ),
    ] {
        let edges = subjects.len() * objects.len();
        for mask in 0..(1usize << edges) {
            let mut data = GeneralizedDataset::new();
            if extra {
                data.insert(
                    GeneralizedQuad::new(
                        iri("a"),
                        iri("q"),
                        literal.clone(),
                        GraphName::DefaultGraph,
                    )
                    .as_ref(),
                );
            }
            for (s, subject) in subjects.iter().enumerate() {
                for (o, object) in objects.iter().enumerate() {
                    if mask & (1 << (s * objects.len() + o)) != 0 {
                        let subject = NamedOrBlankNode::try_from(subject.clone()).unwrap();
                        data.insert(
                            GeneralizedQuad::new(
                                subject,
                                same(),
                                object.clone(),
                                GraphName::DefaultGraph,
                            )
                            .as_ref(),
                        );
                    }
                }
            }
            let measured = Measured {
                data: &data,
                work: Cell::new(0),
            };
            let mut actual = Pending::default();
            apply_rules(&measured, &optimized, &mut actual, true).unwrap();
            assert_eq!(
                actual.0,
                reference(&data, &original),
                "one-step equality mismatch mask={mask}, aliases={extra}"
            );
            // The kernel must not expose its pending symmetry to its own transitive pass.
            assert_eq!(measured.data, &data);
        }
    }
}

#[test]
fn new_symmetry_is_not_visible_to_other_rules_in_the_same_round() {
    let mut data = GeneralizedDataset::new();
    for (s, o) in [("a", "b"), ("b", "c"), ("c", "d")] {
        data.insert(GeneralizedQuad::new(iri(s), same(), iri(o), GraphName::DefaultGraph).as_ref());
    }
    let measured = Measured {
        data: &data,
        work: Cell::new(0),
    };
    let mut pending = Pending::default();
    apply_rules(&measured, &queries(&OWL_RULES[..2]), &mut pending, true).unwrap();
    assert!(pending.0.contains(&GeneralizedQuad::new(
        iri("a"),
        same(),
        iri("c"),
        GraphName::DefaultGraph
    )));
    assert!(!pending.0.contains(&GeneralizedQuad::new(
        iri("a"),
        same(),
        iri("d"),
        GraphName::DefaultGraph
    )));
    assert!(pending.0.contains(&GeneralizedQuad::new(
        iri("b"),
        same(),
        iri("d"),
        GraphName::DefaultGraph
    )));
    let mut fork = GeneralizedDataset::new();
    for o in ["b", "c"] {
        fork.insert(
            GeneralizedQuad::new(iri("a"), same(), iri(o), GraphName::DefaultGraph).as_ref(),
        );
    }
    let measured = Measured {
        data: &fork,
        work: Cell::new(0),
    };
    let mut pending = Pending::default();
    apply_rules(&measured, &queries(&OWL_RULES[..2]), &mut pending, true).unwrap();
    assert!(pending.0.contains(&GeneralizedQuad::new(
        iri("b"),
        same(),
        iri("c"),
        GraphName::DefaultGraph
    )));
}

fn frozen_closure(data: &mut GeneralizedDataset) {
    let mut transaction = data.transaction();
    super::axioms::owl(
        &mut transaction,
        false,
        &mut super::diagnostics::Diagnostics::new(Vec::new(), 4300),
        &super::diagnostic_datatypes::LexicalContext::default(),
    )
    .unwrap();
    // bounded: the fixed former rule inventory, retaining its round semantics.
    let rules: Vec<_> = ORIGINAL
        .iter()
        .chain(
            OWL_RULES
                .iter()
                .filter(|(template, _)| !ORIGINAL.iter().any(|(t, _)| t == template)),
        )
        .copied()
        .collect();
    let mut prepared = queries(&rules);
    // Compare independent equality implementations against the same non-equality
    // semantics; the shared catalogue replaced the former local schema clauses.
    prepared.extend(super::shared::CompiledRule::for_mode("owlrl").unwrap());
    loop {
        let mut pending = Pending::default();
        apply_rules(&transaction, &prepared, &mut pending, false).unwrap();
        pending
            .0
            .extend(super::same_as_reference_tests::one_round(&transaction).unwrap());
        super::traversal::chains(
            &transaction,
            &mut pending,
            &crate::inference::diagnostics::rdf_list::ListWitnesses::new(4300),
        )
        .unwrap();
        super::traversal::keys(
            &transaction,
            &mut pending,
            &crate::inference::diagnostics::rdf_list::ListWitnesses::new(4300),
        )
        .unwrap();
        if pending.0.is_empty() {
            break;
        }
        for quad in pending.0 {
            Graph::insert(&mut transaction, quad).unwrap();
        }
    }
    transaction.commit();
}

#[test]
fn full_closure_matches_frozen_ordinary_rule_engine() {
    for case in 0..4 {
        let mut data = GeneralizedDataset::new();
        for (s, o) in [("a", "b"), ("a", "c"), ("c", "d")] {
            data.insert(
                GeneralizedQuad::new(iri(s), same(), iri(o), GraphName::DefaultGraph).as_ref(),
            );
        }
        data.insert(
            GeneralizedQuad::new(
                iri("a"),
                iri("p"),
                Literal::new_simple_literal("value"),
                GraphName::DefaultGraph,
            )
            .as_ref(),
        );
        if case & 1 != 0 {
            data.insert(
                GeneralizedQuad::new(same(), same(), iri("p"), GraphName::DefaultGraph).as_ref(),
            );
        }
        if case & 2 != 0 {
            data.insert(
                GeneralizedQuad::new(
                    BlankNode::new("blank").unwrap(),
                    same(),
                    Literal::new_simple_literal("value"),
                    GraphName::DefaultGraph,
                )
                .as_ref(),
            );
        }
        let mut reference = data.clone();
        frozen_closure(&mut reference);
        super::expand(&mut data, "owlrl", Default::default()).unwrap();
        for quad in data
            .iter()
            .filter(|quad| !reference.contains(*quad))
            .take(8)
        {
            eprintln!("extra consequence: {quad:?}");
        }
        for quad in reference
            .iter()
            .filter(|quad| !data.contains(*quad))
            .take(8)
        {
            eprintln!("missing consequence: {quad:?}");
        }
        assert!(data == reference, "closure case {case}");
    }
}

#[path = "same_as_dense_tests.rs"]
mod dense;

#[path = "same_as_pending_tests.rs"]
mod pending;
