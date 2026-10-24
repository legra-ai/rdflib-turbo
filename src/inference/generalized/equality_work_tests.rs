//! Work bounds include the shared equality adapter around the ordinary kernel.

use std::cell::{Cell, RefCell};

use oxigraph::model::*;
use oxigraph::sparql::{PreparedSparqlQuery, QueryExplanation, QueryResults};
use serde::Deserialize;

use super::{EqualityRules, Generalized};
use crate::Result;
use crate::inference::Pending;
use crate::inference::backend::Graph;

struct Measured<'data> {
    data: &'data Dataset,
    reads: Cell<usize>,
    // bounded: four fixed seed-query plans.
    plans: RefCell<Vec<QueryExplanation>>,
}

impl<'data> Graph for Measured<'data> {
    fn quads<'a>(
        &'a self,
        s: Option<NamedOrBlankNodeRef<'_>>,
        p: Option<NamedNodeRef<'_>>,
        o: Option<TermRef<'_>>,
    ) -> impl Iterator<Item = Result<Quad>> + use<'a, 'data> {
        self.data
            .quads_for_pattern(s, p, o, Some(GraphNameRef::DefaultGraph))
            .map(|quad| {
                self.reads.set(self.reads.get() + 1);
                Ok(quad.into_owned())
            })
    }

    fn query(&self, query: PreparedSparqlQuery) -> Result<QueryResults<'_>> {
        let (result, plan) = query
            .on_queryable_dataset(self.data)
            .compute_statistics()
            .explain();
        self.plans.borrow_mut().push(plan);
        Ok(result?)
    }

    fn insert(&mut self, _: Quad) -> Result<()> {
        panic!("round reads must remain immutable")
    }
}

#[derive(Deserialize)]
struct Explanation {
    plan: Plan,
}

#[derive(Deserialize)]
struct Plan {
    #[serde(rename = "number of results")]
    results: usize,
    children: Vec<Self>,
}

impl Plan {
    fn largest(&self) -> usize {
        self.children
            .iter()
            .map(Self::largest)
            .fold(self.results, usize::max)
    }
}

#[test]
fn a_closed_named_clique_with_one_literal_keeps_quadratic_adapter_work() {
    let same = NamedNode::new("http://www.w3.org/2002/07/owl#sameAs").unwrap();
    let literal = Literal::new_simple_literal("shared");
    let equality = EqualityRules::try_new().unwrap();
    for size in [8, 16, 32] {
        let mut data = Dataset::new();
        let mut generalized = Generalized::default();
        for left in 0..size {
            let source = NamedNode::new(format!("urn:{left}")).unwrap();
            for right in 0..size {
                data.insert(&Quad::new(
                    source.clone(),
                    same.clone(),
                    NamedNode::new(format!("urn:{right}")).unwrap(),
                    GraphName::DefaultGraph,
                ));
            }
            data.insert(&Quad::new(
                source.clone(),
                same.clone(),
                literal.clone(),
                GraphName::DefaultGraph,
            ));
            generalized.insert([literal.clone().into(), same.clone().into(), source.into()]);
        }
        generalized.insert([
            literal.clone().into(),
            same.clone().into(),
            literal.clone().into(),
        ]);
        let measured = Measured {
            data: &data,
            reads: Cell::new(0),
            plans: RefCell::default(),
        };
        equality
            .apply(
                &generalized,
                &measured,
                &mut Pending::default(),
                &mut Generalized::default(),
            )
            .unwrap();
        assert!(
            measured.reads.get() <= 32 * data.len(),
            "generalized equality read {} rows for {} input triples",
            measured.reads.get(),
            data.len()
        );
        for (index, plan) in measured.plans.into_inner().into_iter().enumerate() {
            // bounded: one finite prepared-query explanation, independent of RDF size.
            let mut bytes = Vec::new();
            plan.write_in_json(&mut bytes).unwrap();
            let explanation: Explanation = serde_json::from_slice(&bytes).unwrap();
            assert!(
                explanation.plan.largest() <= 8 * data.len(),
                "seed {index} enumerated {} witnesses for {} triples (size {size})",
                explanation.plan.largest(),
                data.len()
            );
        }
    }
}
