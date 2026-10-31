//! Baseline report-render overlap; no kernel or wire changes and no RSS claim.
use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;

use oxigraph::io::RdfFormat;
use oxigraph::model::*;
use oxigraph::store::{ReadSnapshot, Store};
use oxrdf::dataset::CanonicalizationAlgorithm;
use pyshacl_turbo_engine::{
    ComparisonFailure, ComparisonOperator, ComparisonRole, CompiledShapes, CoreComparisonPolicy,
    GraphRead, IndexedShapesSource, Options, ValidationReport, ValueComparisonPolicy,
};
use serde::Serialize;

#[derive(Default)]
struct RenderTap {
    calls: Cell<usize>,
    live: Cell<usize>,
    preceding_peak: Cell<usize>,
    cache_payloads: Cell<usize>,
}

struct ObservedSnapshot<'a> {
    source: ReadSnapshot,
    tap: &'a RenderTap,
}

#[derive(Debug)]
struct NoNumericHost;

impl ValueComparisonPolicy for NoNumericHost {
    fn compare_numeric(
        &self,
        _: ComparisonRole,
        _: ComparisonOperator,
        _: &Literal,
        _: &Literal,
    ) -> Result<bool, ComparisonFailure> {
        panic!("NodeKind baseline must not invoke numeric/Decimal policy")
    }
}

impl<'tap> GraphRead for ObservedSnapshot<'tap> {
    fn matching_quads<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = pyshacl_turbo_engine::Result<GeneralizedQuad>> + use<'a, 'tap> {
        let focus = matches!(subject, Some(TermRef::BlankNode(node))
            if node.as_str().starts_with("failure"));
        if focus && predicate.is_none() && object.is_none() && graph.is_none() {
            if self.tap.calls.get() == 0 {
                self.tap
                    .cache_payloads
                    .set(super::REPORT_CACHE_PAYLOAD_LIVE.load(Relaxed));
                let live = super::LIVE.load(Relaxed);
                self.tap.live.set(live);
                self.tap.preceding_peak.set(super::PEAK.swap(live, Relaxed));
            }
            self.tap.calls.set(self.tap.calls.get() + 1);
        }
        GraphRead::matching_quads(&self.source, subject, predicate, object, graph)
    }

    fn named_graphs(
        &self,
    ) -> impl Iterator<Item = pyshacl_turbo_engine::Result<NamedOrBlankNode>> + '_ {
        GraphRead::named_graphs(&self.source)
    }

    fn contains_named_graph(&self, graph: &NamedOrBlankNode) -> pyshacl_turbo_engine::Result<bool> {
        GraphRead::contains_named_graph(&self.source, graph)
    }
}

#[derive(Debug, Serialize)]
struct Observation {
    cache_active: bool,
    conforming: bool,
    live_before: usize,
    live_after: usize,
    first_report_delta: Option<usize>,
    first_report_cache_payloads_delta: Option<isize>,
    pre_report_peak_delta: Option<usize>,
    render_peak_delta: Option<usize>,
    overall_peak_delta: usize,
    allocated: usize,
    calls: usize,
    report_reads: usize,
    report_quads: usize,
}

fn shapes(cache_active: bool, conforming: bool) -> CompiledShapes {
    let source = format!(
        "@prefix sh: <http://www.w3.org/ns/shacl#> .
         @prefix ex: <urn:report-overlap:> .
         ex:CacheShape a sh:NodeShape; sh:order 0; sh:targetClass ex:Cache;
           sh:deactivated {}; sh:nodeKind sh:BlankNode .
         ex:FailureShape a sh:NodeShape; sh:order 1; sh:targetClass ex:Failure;
           sh:nodeKind sh:{} .",
        !cache_active,
        if conforming { "BlankNode" } else { "IRI" }
    );
    CompiledShapes::compile(
        IndexedShapesSource::parse_graph(&source, RdfFormat::Turtle, None).unwrap(),
        false,
    )
    .unwrap()
}

fn data(targets: usize, failures: usize, width: usize) -> Store {
    let store = Store::new().unwrap();
    let rdf_type = oxigraph::model::vocab::rdf::TYPE;
    let key = NamedNode::new("urn:report-overlap:key").unwrap();
    let cache = NamedNode::new("urn:report-overlap:Cache").unwrap();
    let failure = NamedNode::new("urn:report-overlap:Failure").unwrap();
    let padding = "x".repeat(width);
    for i in 0..targets {
        let node = BlankNode::new(format!("cache{i:08}_{padding}")).unwrap();
        store
            .insert(&Quad::new(
                node,
                rdf_type,
                cache.clone(),
                GraphName::DefaultGraph,
            ))
            .unwrap();
    }
    for i in 0..failures {
        let node = BlankNode::new(format!("failure{i:08}_{padding}")).unwrap();
        store
            .insert(&Quad::new(
                node.clone(),
                rdf_type,
                failure.clone(),
                GraphName::DefaultGraph,
            ))
            .unwrap();
        store
            .insert(&Quad::new(
                node,
                key.clone(),
                NamedNode::new(format!("urn:failure-key:{i:08}")).unwrap(),
                GraphName::DefaultGraph,
            ))
            .unwrap();
    }
    store
}

fn measure(store: &Store, cache_active: bool, conforming: bool) -> (ValidationReport, Observation) {
    let program = shapes(cache_active, conforming);
    let tap = RenderTap::default();
    let source = ObservedSnapshot {
        source: store.read_snapshot(),
        tap: &tap,
    };
    let options = Options {
        comparison: CoreComparisonPolicy::Host(Arc::new(NoNumericHost)),
        ..Options::default()
    };
    let payloads_before = super::REPORT_CACHE_PAYLOAD_LIVE.load(Relaxed);
    let (report, allocation) =
        super::measure_requested(|| program.validate_staged(&source, options).unwrap());
    let render = (tap.calls.get() != 0).then_some(allocation.peak);
    let preceding = render.map(|_| tap.preceding_peak.get());
    let overall = allocation.peak.max(preceding.unwrap_or(0));
    let observation = Observation {
        cache_active,
        conforming,
        live_before: allocation.live_before,
        live_after: allocation.live_after,
        first_report_delta: render.map(|_| tap.live.get() - allocation.live_before),
        first_report_cache_payloads_delta: render
            .map(|_| tap.cache_payloads.get() as isize - payloads_before as isize),
        pre_report_peak_delta: preceding.map(|p| p - allocation.live_before),
        render_peak_delta: render.map(|p| p - allocation.live_before),
        overall_peak_delta: overall - allocation.live_before,
        allocated: allocation.allocated,
        calls: allocation.calls,
        report_reads: tap.calls.get(),
        report_quads: report.graph.len(),
    };
    (report, observation)
}

fn assert_same(mut left: ValidationReport, mut right: ValidationReport) {
    assert_eq!(left.conforms, right.conforms);
    assert_eq!(left.text, right.text);
    assert_eq!(left.recursion_warnings, right.recursion_warnings);
    // bounded: only these fixtures' report graphs, outside the measured interval.
    left.graph.canonicalize(CanonicalizationAlgorithm::Unstable);
    right
        .graph
        .canonicalize(CanonicalizationAlgorithm::Unstable);
    assert_eq!(left.graph, right.graph);
}

#[test]
#[ignore = "isolated existing requested-allocation counters; baseline experiment only"]
fn profile_report_cache_overlap_baseline() {
    let warm = data(8, 2, 8);
    drop(measure(&warm, false, false));
    drop(warm);
    for (targets, width) in [(100, 8), (1000, 8), (1000, 256), (10000, 8), (10000, 256)] {
        let failures = targets.min(1000);
        let store = data(targets, failures, width);
        let (inactive, inactive_bytes) = measure(&store, false, false);
        let (active, active_bytes) = measure(&store, true, false);
        assert!(!inactive.conforms);
        assert!(inactive_bytes.report_reads >= failures);
        assert!(active_bytes.report_reads >= failures);
        assert_same(inactive, active);
        let (inactive_ok, inactive_ok_bytes) = measure(&store, false, true);
        let (active_ok, active_ok_bytes) = measure(&store, true, true);
        assert!(inactive_ok.conforms && active_ok.conforms);
        assert_eq!(inactive_ok.graph.len(), 2);
        assert_eq!(active_ok.graph.len(), 2);
        assert_eq!(
            inactive_ok_bytes.report_reads, 0,
            "tap fired without report descriptions"
        );
        assert_eq!(
            active_ok_bytes.report_reads, 0,
            "tap fired during conforming evaluation"
        );
        assert_same(inactive_ok, active_ok);
        println!(
            "REPORT_OVERLAP_BASELINE {}",
            serde_json::to_string(&(
                targets,
                width,
                failures,
                inactive_bytes,
                active_bytes,
                inactive_ok_bytes,
                active_ok_bytes
            ))
            .unwrap()
        );
    }
}

#[test]
#[ignore = "isolated requested-allocation regression; one process and one thread"]
fn report_render_does_not_retain_target_cache_width() {
    let warm = data(8, 2, 8);
    drop(measure(&warm, false, false));
    drop(warm);
    let identifier = format!("cache{:08}_{}", 0, "x".repeat(256));
    assert_eq!(identifier.len(), super::REPORT_CACHE_PAYLOAD_SIZE);
    let before = super::REPORT_CACHE_PAYLOAD_LIVE.load(Relaxed);
    let cloned = identifier.clone();
    assert_eq!(super::REPORT_CACHE_PAYLOAD_LIVE.load(Relaxed), before + 1);
    drop(cloned);
    assert_eq!(super::REPORT_CACHE_PAYLOAD_LIVE.load(Relaxed), before);
    let mut excess = [0isize; 2];
    let mut retained_payloads = 0;
    for (index, width) in [8, 256].into_iter().enumerate() {
        let store = data(10000, 1000, width);
        let (inactive, inactive_bytes) = measure(&store, false, false);
        let (active, active_bytes) = measure(&store, true, false);
        assert_same(inactive, active);
        if width == 256 {
            retained_payloads = active_bytes.first_report_cache_payloads_delta.unwrap()
                - inactive_bytes.first_report_cache_payloads_delta.unwrap();
        }
        excess[index] = active_bytes.first_report_delta.unwrap() as isize
            - inactive_bytes.first_report_delta.unwrap() as isize;
    }
    let retained_width_slope = excess[1] - excess[0];
    println!("REPORT_CACHE_WIDTH_SLOPE {retained_width_slope} {excess:?}");
    println!("REPORT_CACHE_RETAINED_270_BYTE_PAYLOADS {retained_payloads}");
    assert_eq!(
        retained_payloads, 0,
        "completed target-cache payload allocations must not remain live during report rendering"
    );
}
