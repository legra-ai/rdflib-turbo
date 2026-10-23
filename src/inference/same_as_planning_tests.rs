//! Planner resource contracts use actual indexed reads, independent of wall time.
use std::cell::Cell;

use oxigraph::model::*;
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults};

use super::super::{Graph, Pending, Rows};
use crate::Result;

struct Reads<'a> {
    data: &'a Dataset,
    incoming_calls: Cell<usize>,
    incoming_rows: Cell<usize>,
}

impl Graph for Reads<'_> {
    fn quads<'a>(
        &'a self,
        s: Option<NamedOrBlankNodeRef<'a>>,
        p: Option<NamedNodeRef<'a>>,
        o: Option<TermRef<'a>>,
    ) -> impl Iterator<Item = Result<Quad>> + 'a {
        let incoming = s.is_none() && o.is_some();
        if incoming {
            self.incoming_calls.set(self.incoming_calls.get() + 1);
        }
        self.data
            .quads_for_pattern(s, p, o, Some(GraphNameRef::DefaultGraph))
            .map(move |quad| {
                if incoming {
                    self.incoming_rows.set(self.incoming_rows.get() + 1);
                }
                Ok(quad.into_owned())
            })
    }
    fn query(&self, _: PreparedSparqlQuery) -> Result<QueryResults<'_>> {
        panic!("planner must use indexed reads")
    }
    fn insert(&mut self, _: Quad) -> Result<()> {
        panic!("planner must not publish facts")
    }
}

fn node(index: usize) -> NamedNode {
    NamedNode::new(format!("urn:planning:{index}")).unwrap()
}

fn predicate() -> NamedNode {
    NamedNode::new("http://www.w3.org/2002/07/owl#sameAs").unwrap()
}

fn add(data: &mut Dataset, left: usize, right: usize) {
    data.insert(
        Quad::new(
            node(left),
            predicate(),
            node(right),
            GraphName::DefaultGraph,
        )
        .as_ref(),
    );
}

fn pairs(data: &mut Dataset, offset: usize, nodes: usize) {
    for start in (offset..offset + nodes).step_by(2) {
        for a in 0..2 {
            for b in 0..2 {
                add(data, start + a, start + b);
            }
        }
    }
}

fn inspect(data: &Dataset, check: impl FnOnce(&Rows, &Reads<'_>)) {
    let reads = Reads {
        data,
        incoming_calls: Cell::new(0),
        incoming_rows: Cell::new(0),
    };
    let rows = Rows::new(&reads, &predicate(), &mut Pending::default()).unwrap();
    check(&rows, &reads);
}

#[test]
fn balanced_cliques_and_closed_pairs_allocate_no_component_index() {
    assert_eq!(
        std::mem::size_of::<super::super::RowGroup>(),
        std::mem::size_of::<Vec<NamedOrBlankNode>>()
    );
    let mut clique = Dataset::new();
    for left in 0..32 {
        for right in 0..32 {
            add(&mut clique, left, right);
        }
    }
    let mut separate_pairs = Dataset::new();
    pairs(&mut separate_pairs, 0, 5000);
    for data in [&clique, &separate_pairs] {
        inspect(data, |rows, reads| {
            assert_eq!(
                rows.components.members.capacity(),
                0,
                "balanced input allocated a component index"
            );
            assert_eq!(
                rows.components.dense.capacity(),
                0,
                "balanced input allocated per-group dense flags"
            );
            assert_eq!(
                reads.incoming_calls.get(),
                0,
                "balanced input explored weak connectivity"
            );
        });
    }
}

#[test]
fn near_closed_search_does_not_index_unrelated_closed_pairs() {
    let mut data = Dataset::new();
    for left in 0..16 {
        for right in 0..16 {
            if left != right {
                add(&mut data, left, right);
            }
        }
    }
    pairs(&mut data, 100, 5000);
    inspect(&data, |rows, reads| {
        assert_eq!(
            rows.components.members.iter().map(Vec::len).sum::<usize>(),
            16
        );
        assert!(
            reads.incoming_calls.get() <= 16,
            "unrelated components were explored"
        );
        assert!(reads.incoming_rows.get() <= 16 * 15);
    });
}

#[test]
fn oversized_branching_component_stops_at_seed_degree_bound() {
    let mut data = Dataset::new();
    add(&mut data, 0, 1);
    add(&mut data, 0, 2);
    for index in 3..5003 {
        add(&mut data, index, 0);
    }
    inspect(&data, |rows, reads| {
        assert_eq!(
            rows.components.members.capacity(),
            0,
            "oversized sparse component retained an index"
        );
        assert!(reads.incoming_calls.get() <= 4);
        assert!(
            reads.incoming_rows.get() <= 4,
            "degree-two seed scanned the entire incoming star"
        );
    });
}

#[test]
fn completed_component_classifies_balanced_dense_rows_too() {
    let mut data = Dataset::new();
    for left in 0..3 {
        for right in [0, 1, 3] {
            add(&mut data, left, right);
        }
    }
    add(&mut data, 3, 0);
    add(&mut data, 3, 2);
    inspect(&data, |rows, _| {
        assert!(rows.dense(&node(0).into()));
        assert!(!rows.dense(&node(3).into()));
    });
}
