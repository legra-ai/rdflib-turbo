//! Existing-head probe budgets for immutable one-round equality composition.
use std::cell::Cell;

use oxigraph::model::*;
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults};

use super::{Graph, Pending, Rows};
use crate::Result;

struct ExistingHeads<'a> {
    data: &'a Dataset,
    probes: Cell<usize>,
}

impl Graph for ExistingHeads<'_> {
    fn quads<'a>(
        &'a self,
        s: Option<NamedOrBlankNodeRef<'a>>,
        p: Option<NamedNodeRef<'a>>,
        o: Option<TermRef<'a>>,
    ) -> impl Iterator<Item = Result<Quad>> + 'a {
        self.data
            .quads_for_pattern(s, p, o, Some(GraphNameRef::DefaultGraph))
            .map(|quad| Ok(quad.into_owned()))
    }

    fn contains(&self, quad: &Quad) -> Result<bool> {
        let present = self.data.contains(quad);
        if present {
            self.probes.set(self.probes.get() + 1);
        }
        Ok(present)
    }

    fn query(&self, _: PreparedSparqlQuery) -> Result<QueryResults<'_>> {
        panic!("equality composition uses indexed source reads")
    }

    fn insert(&mut self, _: Quad) -> Result<()> {
        panic!("one round must not publish into its immutable source")
    }
}

fn node(index: usize) -> NamedNode {
    NamedNode::new(format!("urn:difference:{index}")).unwrap()
}

#[test]
fn ring_composition_never_probes_already_present_heads() {
    let predicate = NamedNode::new("http://www.w3.org/2002/07/owl#sameAs").unwrap();
    for size in [64, 128] {
        let mut data = Dataset::new();
        for index in 0..size {
            data.insert(
                Quad::new(
                    node(index),
                    predicate.clone(),
                    node((index + 1) % size),
                    GraphName::DefaultGraph,
                )
                .as_ref(),
            );
        }
        // bounded: these rings have at most128 nodes and128² closure facts.
        for round in 0..16 {
            let reads = ExistingHeads {
                data: &data,
                probes: Cell::new(0),
            };
            let mut pending = Pending::default();
            let rows = Rows::new(&reads, &predicate, &mut pending).unwrap();
            reads.probes.set(0);
            rows.extend(&reads, &predicate, &mut pending).unwrap();
            assert_eq!(
                reads.probes.get(),
                0,
                "size={size} round={round} probed existing equality heads"
            );
            if pending.0.is_empty() {
                assert_eq!(data.len(), size * size);
                break;
            }
            assert!(round < 15);
            for quad in pending.0 {
                assert!(!data.contains(&quad));
                data.insert(quad.as_ref());
            }
        }
    }
}
