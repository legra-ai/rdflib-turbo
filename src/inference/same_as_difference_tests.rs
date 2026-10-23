//! Existing-head probe budgets for immutable one-round equality composition.
use std::cell::Cell;

use oxigraph::model::*;
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults};

use super::{Graph, Pending, Rows, SubjectRow};
use crate::Result;

struct ExistingHeads<'a> {
    data: &'a Dataset,
    probes: Cell<usize>,
    work: Cell<usize>,
    incoming: Cell<usize>,
    full_scans: Cell<usize>,
}

impl Graph for ExistingHeads<'_> {
    fn quads<'a>(
        &'a self,
        s: Option<NamedOrBlankNodeRef<'a>>,
        p: Option<NamedNodeRef<'a>>,
        o: Option<TermRef<'a>>,
    ) -> impl Iterator<Item = Result<Quad>> + 'a {
        self.work.set(self.work.get() + 1);
        if s.is_none() && o.is_none() {
            self.full_scans.set(self.full_scans.get() + 1);
        }
        if s.is_none() && o.is_some() {
            self.incoming.set(self.incoming.get() + 1);
        }
        self.data
            .quads_for_pattern(s, p, o, Some(GraphNameRef::DefaultGraph))
            .map(|quad| {
                self.work.set(self.work.get() + 1);
                Ok(quad.into_owned())
            })
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
                work: Cell::new(0),
                incoming: Cell::new(0),
                full_scans: Cell::new(0),
            };
            let mut pending = Pending::default();
            let mut rows = Rows::new(&reads, &predicate, &mut pending).unwrap();
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

#[test]
fn disjoint_closed_groups_do_not_repeat_incoming_row_reads() {
    let predicate = NamedNode::new("http://www.w3.org/2002/07/owl#sameAs").unwrap();
    const GROUPS: usize = 32;
    for size in [1, 2, 16] {
        let mut data = Dataset::new();
        for group in 0..GROUPS {
            for left in 0..size {
                for right in 0..size {
                    data.insert(
                        Quad::new(
                            node(group * size + left),
                            predicate.clone(),
                            node(group * size + right),
                            GraphName::DefaultGraph,
                        )
                        .as_ref(),
                    );
                }
            }
        }
        let reads = ExistingHeads {
            data: &data,
            probes: Cell::new(0),
            work: Cell::new(0),
            incoming: Cell::new(0),
            full_scans: Cell::new(0),
        };
        let mut pending = Pending::default();
        let mut rows = Rows::new(&reads, &predicate, &mut pending).unwrap();
        assert_eq!(rows.groups.len(), GROUPS);
        assert_eq!(
            reads.full_scans.get(),
            1,
            "row preparation added a source pass"
        );
        reads.work.set(0);
        reads.incoming.set(0);
        rows.extend(&reads, &predicate, &mut pending).unwrap();
        assert!(pending.0.is_empty());
        // bounded: one outgoing read and one representative row per exact group.
        let budget = GROUPS * (1 + size);
        assert_eq!(reads.work.get(), budget, "closed group size={size}");
        assert_eq!(reads.incoming.get(), 0, "closed group size={size}");
        assert_eq!(reads.full_scans.get(), 1, "composition added a source pass");
    }
}

#[test]
fn subject_row_annotation_retains_one_machine_word_and_exact_indices() {
    assert_eq!(
        std::mem::size_of::<SubjectRow>(),
        std::mem::size_of::<usize>()
    );
    assert_eq!(
        std::mem::align_of::<SubjectRow>(),
        std::mem::align_of::<usize>()
    );
    for index in [0, 1, 17, usize::MAX / 2] {
        let mut row = SubjectRow::new(index);
        assert_eq!(row.group(), index);
        assert!(!row.has_cross_group_incoming());
        row.mark_cross_group_incoming();
        row.mark_cross_group_incoming();
        assert_eq!(row.group(), index);
        assert!(row.has_cross_group_incoming());
    }
}

#[test]
#[should_panic(expected = "equality row index exceeds its subject slot")]
fn subject_row_index_overflow_fails_explicitly() {
    SubjectRow::new(usize::MAX);
}
