//! Work and semantic barriers for conservative catalogue scheduling.
use oxrdf::GeneralizedDataset;
use std::cell::Cell;

use oxigraph::model::*;
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults};

use super::{Graph, expand_inner};
use crate::Result;

#[derive(Default)]
struct Measured {
    data: GeneralizedDataset,
    queries: Cell<usize>,
    rounds: Cell<usize>,
    queried_since_insert: Cell<bool>,
}

impl Graph for Measured {
    fn quads<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
    ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a> {
        self.data
            .quads_for_pattern(subject, predicate, object, Some(GraphNameRef::DefaultGraph))
            .map(|quad| Ok(quad.into_owned()))
    }

    fn query(&self, query: PreparedSparqlQuery) -> Result<QueryResults<'_>> {
        self.queries.set(self.queries.get() + 1);
        if !self.queried_since_insert.replace(true) {
            self.rounds.set(self.rounds.get() + 1);
        }
        Ok(query.on_queryable_dataset(&self.data).execute()?)
    }

    fn inference_heads(
        &self,
        query: PreparedSparqlQuery,
    ) -> Result<oxigraph::sparql::QueryTripleIter<'_, GeneralizedTriple>> {
        self.queries.set(self.queries.get() + 1);
        if !self.queried_since_insert.replace(true) {
            self.rounds.set(self.rounds.get() + 1);
        }
        Ok(query
            .on_queryable_dataset(&self.data)
            .execute_inference_construct()?)
    }

    fn cleanup_blank_predicates(&mut self) -> Result<()> {
        let mut transaction = self.data.transaction();
        transaction.remove_where(|quad| quad.predicate.is_blank_node());
        transaction.commit();
        Ok(())
    }

    fn insert(&mut self, quad: GeneralizedQuad) -> Result<()> {
        self.data.insert(quad.as_ref());
        self.queried_since_insert.set(false);
        Ok(())
    }
}

fn iri(value: &str) -> NamedNode {
    NamedNode::new(format!("urn:{value}")).unwrap()
}

#[test]
fn stable_schema_clauses_are_not_reevaluated_for_unrelated_publications() {
    let mut measured = Measured::default();
    let subproperty = NamedNode::new("http://www.w3.org/2000/01/rdf-schema#subPropertyOf").unwrap();
    for (subject, predicate, object) in [
        (iri("s"), iri("p"), iri("o")),
        (iri("p"), subproperty.clone(), iri("q")),
        (iri("q"), subproperty.clone(), iri("r")),
    ] {
        measured.data.insert(
            GeneralizedQuad::new(subject, predicate, object, GraphName::DefaultGraph).as_ref(),
        );
    }
    expand_inner(&mut measured, "rdfs", Default::default()).unwrap();
    assert!(measured.data.contains(
        GeneralizedQuad::new(iri("s"), iri("r"), iri("o"), GraphName::DefaultGraph).as_ref()
    ));
    let rounds = measured.rounds.get();
    assert!(rounds > 1, "fixture must publish more than one round");
    // One initialization query plus all twelve clauses in each round was the
    // former behavior. Require actual skipped work; type/schema predicates can
    // legitimately change together, so a fixed per-round saving is unsound.
    let budget = 12 * rounds;
    assert!(
        measured.queries.get() <= budget,
        "{} queries across {rounds} rounds exceed conservative scheduling budget {budget}",
        measured.queries.get()
    );
}

#[test]
fn scheduled_flat_rounds_match_full_queries_with_late_list_and_variable_predicates() {
    use super::{Pending, apply_rules, scheduling::Schedule, shared::CompiledRule};

    for mode in ["rdfs", "owlrl", "both"] {
        let mut data = Measured::default();
        let rdf = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
        let rdfs = "http://www.w3.org/2000/01/rdf-schema#";
        let owl = "http://www.w3.org/2002/07/owl#";
        let named = |value: &str| NamedNode::new(value).unwrap();
        let additions = [
            GeneralizedQuad::new(iri("s"), iri("p"), iri("o"), GraphName::DefaultGraph),
            GeneralizedQuad::new(
                iri("p"),
                named(&format!("{rdfs}domain")),
                iri("C"),
                GraphName::DefaultGraph,
            ),
            GeneralizedQuad::new(
                iri("C"),
                named(&format!("{owl}unionOf")),
                iri("list"),
                GraphName::DefaultGraph,
            ),
            GeneralizedQuad::new(
                iri("list"),
                named(&format!("{rdf}rest")),
                iri("tail"),
                GraphName::DefaultGraph,
            ),
            GeneralizedQuad::new(
                iri("tail"),
                named(&format!("{rdf}first")),
                iri("D"),
                GraphName::DefaultGraph,
            ),
            GeneralizedQuad::new(
                iri("s"),
                named(&format!("{rdf}type")),
                iri("D"),
                GraphName::DefaultGraph,
            ),
            GeneralizedQuad::new(
                iri("q"),
                named(&format!("{rdfs}subPropertyOf")),
                iri("p"),
                GraphName::DefaultGraph,
            ),
            GeneralizedQuad::new(iri("x"), iri("q"), iri("y"), GraphName::DefaultGraph),
        ];
        let queries = CompiledRule::for_mode(mode).unwrap();
        let mut schedule = Schedule::for_mode(mode).unwrap();
        for round in 0..24 {
            let mut expected = Pending::default();
            apply_rules(&data, &queries, &mut expected, false).unwrap();
            let mut actual = Pending::default();
            schedule.apply(&data, &mut actual).unwrap();
            assert_eq!(actual.0, expected.0, "{mode}, immutable round {round}");
            let external = additions.get(round).cloned();
            if actual.0.is_empty() && external.is_none() {
                break;
            }
            let mut dirty = 0;
            for quad in actual.0.into_iter().chain(external) {
                dirty |= schedule.changed(&quad.predicate);
                data.insert(quad).unwrap();
            }
            schedule.advance(dirty);
            assert!(round < 23, "fixture did not converge");
        }
    }
}
