//! Pending facts probe their immutable read snapshot once per distinct new head.
use super::*;

#[test]
fn duplicate_new_head_checks_storage_only_once() {
    let data = Dataset::new();
    let measured = Measured {
        data: &data,
        work: Cell::new(0),
    };
    let quad = Quad::new(iri("left"), same(), iri("right"), GraphName::DefaultGraph);
    let mut pending = Pending::default();
    pending.push(&measured, quad.clone()).unwrap();
    pending.push(&measured, quad.clone()).unwrap();
    assert_eq!(pending.0, HashSet::from([quad]));
    assert_eq!(
        measured.work.get(),
        1,
        "duplicate new head repeated its storage probe"
    );
}

#[test]
fn equality_ring_round_work_is_recorded_until_complete() {
    for size in [64, 128] {
        let mut data = Dataset::new();
        for index in 0..size {
            data.insert(
                Quad::new(
                    iri(&format!("ring:{index}")),
                    same(),
                    iri(&format!("ring:{}", (index + 1) % size)),
                    GraphName::DefaultGraph,
                )
                .as_ref(),
            );
        }
        // bounded: these synthetic rings have at most128 nodes and128² closure facts.
        for round in 0..16 {
            let measured = Measured {
                data: &data,
                work: Cell::new(0),
            };
            let mut pending = Pending::default();
            super::super::same_as::apply(&measured, &mut pending).unwrap();
            eprintln!(
                "ring_size={size} round={round} facts={} new={} work={}",
                data.len(),
                pending.0.len(),
                measured.work.get()
            );
            if pending.0.is_empty() {
                assert_eq!(data.len(), size * size);
                break;
            }
            assert!(
                round < 15,
                "bounded ring did not reach its equality closure"
            );
            for quad in pending.0 {
                assert!(!data.contains(&quad));
                data.insert(quad.as_ref());
            }
        }
    }
}

struct FirstProbeFails(Cell<usize>);

impl Graph for FirstProbeFails {
    fn quads<'a>(
        &'a self,
        _: Option<NamedOrBlankNodeRef<'_>>,
        _: Option<NamedNodeRef<'_>>,
        _: Option<TermRef<'_>>,
    ) -> impl Iterator<Item = Result<Quad>> + use<'a> {
        std::iter::empty()
    }

    fn contains(&self, _: &Quad) -> Result<bool> {
        self.0.set(self.0.get() + 1);
        if self.0.get() == 1 {
            return Err(crate::Error::Runtime("first probe failed".into()));
        }
        Ok(false)
    }

    fn query(&self, _: PreparedSparqlQuery) -> Result<QueryResults<'_>> {
        panic!("pending storage probes do not execute queries")
    }

    fn insert(&mut self, _: Quad) -> Result<()> {
        panic!("pending storage probes do not publish facts")
    }
}

#[test]
fn failed_probe_does_not_mark_head_pending_or_hide_retry() {
    let data = FirstProbeFails(Cell::new(0));
    let quad = Quad::new(iri("left"), same(), iri("right"), GraphName::DefaultGraph);
    let mut pending = Pending::default();
    let error = pending.push(&data, quad.clone()).unwrap_err();
    assert_eq!(error.to_string(), "first probe failed");
    assert!(pending.0.is_empty());
    pending.push(&data, quad.clone()).unwrap();
    pending.push(&data, quad.clone()).unwrap();
    assert_eq!(pending.0, HashSet::from([quad]));
    assert_eq!(data.0.get(), 2);
}
