//! Actual live requested storage for many one-value selected projections.
use std::any::Any;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

use oxigraph::io::RdfFormat;
use oxigraph::model::*;
use oxigraph::store::{ReadSnapshot, Store};
use pyshacl_turbo_engine::{
    CompiledShapes, GraphRead, IndexedShapesSource, Options, ReportTermRole, SelectedLiteral,
    SelectedTerm, SelectedValueFrame, SelectedValueSource,
};

struct Source {
    snapshot: ReadSnapshot,
    frame: Arc<Frame>,
}

impl std::fmt::Debug for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SelectedObservationSource")
    }
}

#[derive(Debug)]
struct Frame {
    values: usize,
    observations: usize,
    targets: usize,
    acquired: AtomicUsize,
    last_acquisition_live: AtomicUsize,
    first_acquisition_calls: AtomicUsize,
    last_acquisition_calls: AtomicUsize,
}

impl SelectedValueSource for Source {
    fn begin_frame(&self) -> pyshacl_turbo_engine::Result<Arc<dyn SelectedValueFrame>> {
        Ok(self.frame.clone())
    }
}

impl SelectedValueFrame for Frame {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn begin_nested(&self) -> pyshacl_turbo_engine::Result<Arc<dyn SelectedValueFrame>> {
        panic!("This test issuer has no nested shape acquisition")
    }
    fn predicate(
        &self,
        _: &Term,
        _: &NamedNode,
        _: Option<pyshacl_turbo_engine::SelectedSubjectRef<'_>>,
    ) -> pyshacl_turbo_engine::Result<Vec<SelectedTerm>> {
        let acquired = self.acquired.fetch_add(1, Relaxed) + 1;
        if acquired == 1 {
            self.first_acquisition_calls
                .store(super::CALLS.load(Relaxed), Relaxed);
        }
        if acquired == self.targets {
            self.last_acquisition_live
                .store(super::live_requested_bytes(), Relaxed);
            self.last_acquisition_calls
                .store(super::CALLS.load(Relaxed), Relaxed);
        }
        Ok((0..self.values)
            .map(|index| SelectedTerm {
                term: NamedNode::new("urn:value").unwrap().into(),
                slot: (index < self.observations).then_some(index),
            })
            .collect())
    }
    fn sequence(
        &self,
        _: &Term,
        _: pyshacl_turbo_engine::SelectedPathRef<'_>,
        _: Option<pyshacl_turbo_engine::SelectedSubjectRef<'_>>,
    ) -> pyshacl_turbo_engine::Result<Vec<SelectedTerm>> {
        panic!("This predicate allocation issuer does not acquire sequences")
    }
    fn datatype(&self, _: usize, _: &NamedNode) -> pyshacl_turbo_engine::Result<bool> {
        panic!("NodeKind must not read datatype assessment")
    }
    fn retain(&self, _: usize) -> pyshacl_turbo_engine::Result<SelectedLiteral> {
        panic!("conforming values must not acquire report owners")
    }
    fn present(&self, _: usize, _: ReportTermRole) -> pyshacl_turbo_engine::Result<String> {
        panic!("conforming values must not request report text")
    }
    fn close(&self) {}
}

impl GraphRead for Source {
    fn selected_focus_source(&self) -> Option<&dyn pyshacl_turbo_engine::SelectedFocusSource> {
        None
    }
    fn selected_value_source(&self) -> Option<&dyn SelectedValueSource> {
        Some(self)
    }
    fn boolean_object_state(
        &self,
        _: &GeneralizedQuad,
    ) -> pyshacl_turbo_engine::Result<Option<pyshacl_turbo_engine::BuiltinBooleanState>> {
        Ok(None)
    }
    fn matching_quads<'a>(
        &'a self,
        s: Option<TermRef<'_>>,
        p: Option<TermRef<'_>>,
        o: Option<TermRef<'_>>,
        g: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = pyshacl_turbo_engine::Result<GeneralizedQuad>> + use<'a> {
        GraphRead::matching_quads(&self.snapshot, s, p, o, g)
    }
    fn named_graphs(
        &self,
    ) -> impl Iterator<Item = pyshacl_turbo_engine::Result<NamedOrBlankNode>> + '_ {
        GraphRead::named_graphs(&self.snapshot)
    }
    fn contains_named_graph(&self, g: &NamedOrBlankNode) -> pyshacl_turbo_engine::Result<bool> {
        GraphRead::contains_named_graph(&self.snapshot, g)
    }
}

fn measure(program: &CompiledShapes, store: &Store, targets: usize, issue: bool) -> usize {
    measure_batch(program, store, targets, 1, usize::from(issue)).0
}

fn measure_batch(
    program: &CompiledShapes,
    store: &Store,
    targets: usize,
    values: usize,
    observations: usize,
) -> (usize, usize) {
    assert!(observations <= values);
    let frame = Arc::new(Frame {
        values,
        observations,
        targets,
        acquired: AtomicUsize::new(0),
        last_acquisition_live: AtomicUsize::new(0),
        first_acquisition_calls: AtomicUsize::new(0),
        last_acquisition_calls: AtomicUsize::new(0),
    });
    let source = Source {
        snapshot: store.read_snapshot(),
        frame: frame.clone(),
    };
    let (report, cost) = super::measure_requested(|| {
        program
            .validate_staged(&source, Options::default())
            .unwrap()
    });
    assert!(report.conforms);
    assert_eq!(report.graph.len(), 2);
    assert_eq!(report.text, "Validation Report\nConforms: True\n");
    assert!(report.recursion_warnings.is_empty());
    assert_eq!(frame.acquired.load(Relaxed), targets);
    let live = frame.last_acquisition_live.load(Relaxed) - cost.live_before;
    // Exclude target discovery's randomized input-to-sort work and report setup.
    // Both endpoints are the same native-only synchronous acquisition event.
    let acquisition_calls =
        frame.last_acquisition_calls.load(Relaxed) - frame.first_acquisition_calls.load(Relaxed);
    println!(
        "SELECTED_OBSERVATION_REQUESTED targets={targets} values={values} observations={observations} acquisition_live={live} acquisition_calls={acquisition_calls} peak={} allocated={} calls={}",
        cost.peak - cost.live_before,
        cost.allocated,
        cost.calls
    );
    (live, acquisition_calls)
}

#[test]
#[ignore = "isolated requested-allocation regression; one process and one thread"]
fn one_value_frames_retain_one_observation_request_each() {
    let program = CompiledShapes::compile(
        IndexedShapesSource::parse_graph(
            "@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:S> sh:targetSubjectsOf <urn:p>; sh:path <urn:p>; sh:nodeKind sh:IRI.",
            RdfFormat::Turtle,
            None,
        )
        .unwrap(),
        false,
    )
    .unwrap();
    for targets in [100, 10000] {
        let store = Store::new().unwrap();
        for i in 0..targets {
            store
                .insert(&Quad::new(
                    NamedNode::new(format!("urn:s{i}")).unwrap(),
                    NamedNode::new("urn:p").unwrap(),
                    NamedNode::new("urn:value").unwrap(),
                    GraphName::DefaultGraph,
                ))
                .unwrap();
        }
        // The same actual value acquisition and RDF words in both arms isolate
        // only the sparse observation requests retained by completed projections.
        let bare = measure(&program, &store, targets, false);
        let issued = measure(&program, &store, targets, true);
        let observation_bytes =
            std::mem::size_of::<(usize, pyshacl_turbo_engine::BooleanObservation)>();
        assert_eq!(issued - bare, (targets - 1) * observation_bytes);
    }
}

fn small_batch_budget(values: usize) {
    let program = CompiledShapes::compile(
        IndexedShapesSource::parse_graph(
            "@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:S> sh:targetSubjectsOf <urn:p>; sh:path <urn:p>; sh:nodeKind sh:IRI.",
            RdfFormat::Turtle,
            None,
        )
        .unwrap(),
        false,
    )
    .unwrap();
    let targets = 100;
    let store = Store::new().unwrap();
    for index in 0..targets {
        store
            .insert(&Quad::new(
                NamedNode::new(format!("urn:s{index}")).unwrap(),
                NamedNode::new("urn:p").unwrap(),
                NamedNode::new("urn:value").unwrap(),
                GraphName::DefaultGraph,
            ))
            .unwrap();
    }
    let (bare_live, bare_calls) = measure_batch(&program, &store, targets, values, 0);
    let (dense_live, dense_calls) = measure_batch(&program, &store, targets, values, values);
    let entry = std::mem::size_of::<(usize, pyshacl_turbo_engine::BooleanObservation)>();
    // All actual selected rows can contribute one observation at most. Retained
    // requests cover exactly that known bound, with one allocation per projection.
    assert_eq!(dense_live - bare_live, (targets - 1) * values * entry);
    assert_eq!(dense_calls - bare_calls, targets - 1);
    let (sparse_live, sparse_calls) = measure_batch(&program, &store, targets, values, 1);
    assert_eq!(sparse_live - bare_live, (targets - 1) * values * entry);
    assert_eq!(sparse_calls - bare_calls, targets - 1);
}

#[test]
#[ignore = "isolated requested-allocation regression; one process and one thread"]
fn two_value_batches_do_not_reallocate_the_first_observation() {
    small_batch_budget(2);
}

#[test]
#[ignore = "isolated requested-allocation regression; one process and one thread"]
fn three_value_batches_do_not_reallocate_the_first_observation() {
    small_batch_budget(3);
}
