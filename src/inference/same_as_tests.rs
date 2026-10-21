//! Synthetic one-round semantics and deterministic intermediate-work contracts.
use std::cell::Cell;
use std::collections::HashSet;

use oxigraph::model::*;
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults, SparqlEvaluator};
use serde::Deserialize;

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

struct Measured<'a> {
    data: &'a Dataset,
    work: Cell<usize>,
}

#[derive(Deserialize)]
struct Explanation {
    plan: Node,
}
#[derive(Deserialize)]
struct Node {
    #[serde(rename = "number of results")]
    rows: usize,
    children: Vec<Node>,
}
impl Node {
    fn work(&self) -> usize {
        self.rows + self.children.iter().map(Self::work).sum::<usize>()
    }
}

impl Graph for Measured<'_> {
    fn quads<'a>(
        &'a self,
        s: Option<NamedOrBlankNodeRef<'a>>,
        p: Option<NamedNodeRef<'a>>,
        o: Option<TermRef<'a>>,
    ) -> impl Iterator<Item = Result<Quad>> + 'a {
        self.data
            .quads_for_pattern(s, p, o, Some(GraphNameRef::DefaultGraph))
            .map(|q| {
                self.work.set(self.work.get() + 1);
                Ok(q.into_owned())
            })
    }
    fn query(&self, query: PreparedSparqlQuery) -> Result<QueryResults<'_>> {
        let (result, explanation) = query
            .clone()
            .on_queryable_dataset(self.data)
            .compute_statistics()
            .explain();
        let QueryResults::Graph(triples) = result.unwrap() else {
            panic!("expected CONSTRUCT")
        };
        for triple in triples {
            triple.unwrap();
        }
        // bounded: the fixed rule's explanation tree, independent of graph cardinality.
        let mut bytes = Vec::new();
        explanation.write_in_json(&mut bytes).unwrap();
        let explanation: Explanation = serde_json::from_slice(&bytes).unwrap();
        self.work.set(self.work.get() + explanation.plan.work());
        Ok(query.on_queryable_dataset(self.data).execute()?)
    }
    fn insert(&mut self, _: Quad) {
        panic!("round must not mutate its read snapshot")
    }
}

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
fn reference(data: &Dataset, queries: &[PreparedSparqlQuery]) -> HashSet<Quad> {
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
            let quad = Quad::new(t.subject, t.predicate, t.object, GraphName::DefaultGraph);
            if !data.contains(&quad) {
                result.insert(quad);
            }
        }
    }
    result
}

#[test]
fn closed_same_as_clique_has_quadratic_round_work() {
    const N: usize = 12;
    let mut data = Dataset::new();
    for i in 0..N {
        for j in 0..N {
            data.insert(
                Quad::new(
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
            let mut data = Dataset::new();
            if extra {
                data.insert(
                    Quad::new(iri("a"), iri("q"), literal.clone(), GraphName::DefaultGraph)
                        .as_ref(),
                );
            }
            for (s, subject) in subjects.iter().enumerate() {
                for (o, object) in objects.iter().enumerate() {
                    if mask & (1 << (s * objects.len() + o)) != 0 {
                        let subject = NamedOrBlankNode::try_from(subject.clone()).unwrap();
                        data.insert(
                            Quad::new(subject, same(), object.clone(), GraphName::DefaultGraph)
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
    let mut data = Dataset::new();
    for (s, o) in [("a", "b"), ("b", "c"), ("c", "d")] {
        data.insert(Quad::new(iri(s), same(), iri(o), GraphName::DefaultGraph).as_ref());
    }
    let measured = Measured {
        data: &data,
        work: Cell::new(0),
    };
    let mut pending = Pending::default();
    apply_rules(&measured, &queries(&OWL_RULES[..2]), &mut pending, true).unwrap();
    assert!(pending.0.contains(&Quad::new(
        iri("a"),
        same(),
        iri("c"),
        GraphName::DefaultGraph
    )));
    assert!(!pending.0.contains(&Quad::new(
        iri("a"),
        same(),
        iri("d"),
        GraphName::DefaultGraph
    )));
    assert!(pending.0.contains(&Quad::new(
        iri("b"),
        same(),
        iri("d"),
        GraphName::DefaultGraph
    )));
    let mut fork = Dataset::new();
    for o in ["b", "c"] {
        fork.insert(Quad::new(iri("a"), same(), iri(o), GraphName::DefaultGraph).as_ref());
    }
    let measured = Measured {
        data: &fork,
        work: Cell::new(0),
    };
    let mut pending = Pending::default();
    apply_rules(&measured, &queries(&OWL_RULES[..2]), &mut pending, true).unwrap();
    assert!(pending.0.contains(&Quad::new(
        iri("b"),
        same(),
        iri("c"),
        GraphName::DefaultGraph
    )));
}

fn frozen_closure(data: &mut Dataset) {
    let mut transaction = data.insert_transaction();
    super::axioms::owl(&mut transaction, false).unwrap();
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
    let prepared = queries(&rules);
    loop {
        let mut pending = Pending::default();
        apply_rules(&transaction, &prepared, &mut pending, false).unwrap();
        super::traversal::chains(&transaction, &mut pending).unwrap();
        super::traversal::keys(&transaction, &mut pending).unwrap();
        if pending.0.is_empty() {
            break;
        }
        for quad in pending.0 {
            Graph::insert(&mut transaction, quad);
        }
    }
    transaction.commit();
}

#[test]
fn full_closure_matches_frozen_ordinary_rule_engine() {
    for case in 0..4 {
        let mut data = Dataset::new();
        for (s, o) in [("a", "b"), ("a", "c"), ("c", "d")] {
            data.insert(Quad::new(iri(s), same(), iri(o), GraphName::DefaultGraph).as_ref());
        }
        data.insert(
            Quad::new(
                iri("a"),
                iri("p"),
                Literal::new_simple_literal("value"),
                GraphName::DefaultGraph,
            )
            .as_ref(),
        );
        if case & 1 != 0 {
            data.insert(Quad::new(same(), same(), iri("p"), GraphName::DefaultGraph).as_ref());
        }
        if case & 2 != 0 {
            data.insert(
                Quad::new(
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
        assert_eq!(data, reference, "closure case {case}");
    }
}
