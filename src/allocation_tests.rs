//! Requested Rust heap bytes, not allocator arenas, Python allocations or RSS.
//! Run this ignored test alone, with --test-threads=1. Production has no counter.
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

use oxigraph::model::vocab::{rdf, rdfs, xsd};

use super::*;

#[path = "allocation_canonical_profile_tests.rs"]
mod canonical;
#[path = "allocation_thread_tests.rs"]
pub(crate) mod current_thread;
#[path = "allocation_focus_order_tests.rs"]
mod focus_order;
#[path = "allocation_report_overlap_tests.rs"]
mod report_overlap;
#[path = "allocation_report_retirement_tests.rs"]
mod report_retirement;
#[path = "allocation_selected_observation_tests.rs"]
mod selected_observations;
#[path = "allocation_trace_tests.rs"]
pub(crate) mod trace;

struct CountingAllocator;
// Exact cache-ID payload requests for the isolated report regression (test-only).
const REPORT_CACHE_PAYLOAD_SIZE: usize = 270;
static REPORT_CACHE_PAYLOAD_LIVE: AtomicUsize = AtomicUsize::new(0);

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static TOTAL: AtomicUsize = AtomicUsize::new(0);
static CALLS: AtomicUsize = AtomicUsize::new(0);

fn allocated(size: usize) {
    current_thread::allocated(size);
    if size == REPORT_CACHE_PAYLOAD_SIZE {
        REPORT_CACHE_PAYLOAD_LIVE.fetch_add(1, Relaxed);
    }
    let live = LIVE.fetch_add(size, Relaxed) + size;
    PEAK.fetch_max(live, Relaxed);
    TOTAL.fetch_add(size, Relaxed);
    CALLS.fetch_add(1, Relaxed);
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            allocated(layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            allocated(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if layout.size() == REPORT_CACHE_PAYLOAD_SIZE {
            REPORT_CACHE_PAYLOAD_LIVE.fetch_sub(1, Relaxed);
        }
        LIVE.fetch_sub(layout.size(), Relaxed);
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let next = unsafe { System.realloc(pointer, layout, size) };
        if !next.is_null() {
            if layout.size() == REPORT_CACHE_PAYLOAD_SIZE {
                REPORT_CACHE_PAYLOAD_LIVE.fetch_sub(1, Relaxed);
            }
            LIVE.fetch_sub(layout.size(), Relaxed);
            allocated(size);
        }
        next
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn phase(name: &str, quads: usize) {
    let live = LIVE.load(Relaxed);
    let peak = PEAK.swap(live, Relaxed);
    let total = TOTAL.swap(0, Relaxed);
    let calls = CALLS.swap(0, Relaxed);
    println!(
        "MEMORY_PHASE {}",
        json!({"phase":name, "quads":quads, "live_bytes":live, "peak_bytes":peak,
            "allocated_bytes":total, "allocation_calls":calls})
    );
    // Exclude serialization/logging from the next phase's accounting.
    PEAK.store(LIVE.load(Relaxed), Relaxed);
    TOTAL.store(0, Relaxed);
    CALLS.store(0, Relaxed);
}

/// Current requested Rust heap bytes for isolated allocation regressions.
pub(crate) fn live_requested_bytes() -> usize {
    LIVE.load(Relaxed)
}

/// Measure requested bytes for one synchronous operation in an isolated test.
pub(crate) fn measure_allocated<T>(operation: impl FnOnce() -> T) -> (T, usize) {
    TOTAL.store(0, Relaxed);
    let result = operation();
    (result, TOTAL.load(Relaxed))
}

/// Detached counters for one isolated test-only attribution arm.
#[derive(Debug, serde::Serialize)]
pub(crate) struct RequestedAllocation {
    pub live_before: usize,
    pub live_after: usize,
    pub peak: usize,
    pub allocated: usize,
    pub calls: usize,
}

/// Keep input construction and semantic observations outside this scope.
pub(crate) fn measure_requested<T>(operation: impl FnOnce() -> T) -> (T, RequestedAllocation) {
    let live_before = LIVE.load(Relaxed);
    PEAK.store(live_before, Relaxed);
    TOTAL.store(0, Relaxed);
    CALLS.store(0, Relaxed);
    let result = operation();
    let observed = RequestedAllocation {
        live_before,
        live_after: LIVE.load(Relaxed),
        peak: PEAK.load(Relaxed),
        allocated: TOTAL.load(Relaxed),
        calls: CALLS.load(Relaxed),
    };
    (result, observed)
}

#[test]
#[ignore = "global allocation counters require isolated execution"]
fn named_inference_does_not_clone_source_lexical_storage() {
    let mut data = oxrdf::GeneralizedDataset::new();
    let quad = Quad::new(
        NamedNode::new_unchecked("urn:s"),
        NamedNode::new_unchecked("urn:p"),
        Literal::new_simple_literal("x".repeat(64 * 1024)),
        NamedNode::new_unchecked(format!("urn:graph:{}", "g".repeat(64 * 1024))),
    );
    data.insert(&quad);
    let request = inference::InferenceRequest::default();
    let destination = NamedNode::new_unchecked("urn:destination").into();
    let ((delta, _), allocated) = measure_allocated(|| {
        inference::expand_delta(&mut data, "none", Some(destination), request).unwrap()
    });
    assert!(delta.is_empty());
    assert_eq!(data.len(), 1);
    assert!(data.contains(&quad));
    println!("NAMED_INFERENCE allocated_bytes={allocated}");
    assert!(
        allocated < 16 * 1024,
        "Inference cloned source payloads: {allocated}"
    );
}

#[test]
#[ignore = "explicit release-mode profiling, run alone with a fixture and shapes file"]
fn profile_pipeline_allocations() {
    let source = std::fs::read_to_string(std::env::var("MEMORY_DATA").unwrap()).unwrap();
    let shapes = std::fs::read_to_string(std::env::var("MEMORY_SHAPES").unwrap()).unwrap();
    let mode = std::env::var("MEMORY_INFERENCE").unwrap();
    Python::initialize();
    Python::attach(|py| {
        let policy = crate::native_tests::host_regex_policy(py);
        let data = NativeDataset::new();
        phase("setup", 0);
        data.load(py, &source, "turtle", None, true).unwrap();
        phase("parse", data.__len__().unwrap());
        data.infer(py, &mode, None, false, false, None).unwrap();
        phase("inference", data.__len__().unwrap());
        let program = shacl::SharedShapes::new(
            py,
            &shapes,
            false,
            Vec::new(),
            false,
            policy,
            crate::native_tests::host_comparison_policy(py),
        )
        .unwrap();
        phase("compile_shapes", data.__len__().unwrap());
        let report = program.validate(py, &data, "{}", false).unwrap();
        std::hint::black_box(&report);
        phase("validation_native_report", data.__len__().unwrap());
        drop((report, program, data));
        phase("released", 0);
    });
}

/// A deterministic allocation budget, independent of RSS, timing and arena reuse.
/// Baseline inference exceeded 2.8 times the retained Rust heap on this workload.
#[test]
#[ignore = "global counters require this test to run alone"]
fn inference_heap_budget() {
    use std::fmt::Write;
    let mut source = String::from(
        "@prefix ex: <urn:example:> . @prefix owl: <http://www.w3.org/2002/07/owl#> . @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> . ex:Employee rdfs:subClassOf ex:Person . ex:knows a owl:SymmetricProperty .\n",
    );
    for index in 0..1000 {
        writeln!(source, "ex:p{index} a ex:Employee; ex:knows ex:p{}; ex:score {index}; ex:name \"Person {index}\" .", (index + 1) % 1000).unwrap();
    }
    Python::initialize();
    Python::attach(|py| {
        let data = NativeDataset::new();
        data.load(py, &source, "turtle", None, true).unwrap();
        PEAK.store(LIVE.load(Relaxed), Relaxed);
        data.infer(py, "owlrl", None, false, false, None).unwrap();
        let peak = PEAK.load(Relaxed);
        let retained = LIVE.load(Relaxed);
        println!("INFERENCE_HEAP peak_bytes={peak} retained_bytes={retained}");
        assert!(
            peak < retained * 2,
            "Inference heap budget exceeded: peak={peak}, retained={retained}"
        );
        // bounded: the fixed 1,000-subject fixture's complete closure, captured after budget checks.
        let physical_quads = || {
            data.inner
                .read()
                .unwrap()
                .iter()
                .map(GeneralizedQuadRef::into_owned)
                .collect::<HashSet<_>>()
        };
        let first = physical_quads();
        data.infer(py, "owlrl", None, false, false, None).unwrap();
        let second = physical_quads();
        assert!(
            first.is_subset(&second),
            "Repeated OWL-RL must not remove facts"
        );
        // bounded: the pinned original's second call adds one datatype row per fixture integer.
        let expected_added = (0..1000)
            .map(|index| {
                GeneralizedQuad::new(
                    Literal::new_typed_literal(index.to_string(), xsd::INTEGER),
                    rdf::TYPE,
                    rdfs::DATATYPE,
                    GraphName::DefaultGraph,
                )
            })
            .collect::<HashSet<_>>();
        let added = second.difference(&first).cloned().collect::<HashSet<_>>();
        assert_eq!(added, expected_added);
    });
}

/// Repeated lexical values must be interned as parsed, never retained per input row.
#[test]
#[ignore = "global peak counters require this test to run alone"]
fn parse_heap_budget() {
    let lexical = "x".repeat(64 * 1024);
    let mut source = String::new();
    for index in 0..512 {
        source.push_str(&format!("<urn:s{index}> <urn:p> \"{lexical}\" .\n"));
    }
    Python::initialize();
    Python::attach(|py| {
        let data = NativeDataset::new();
        PEAK.store(LIVE.load(Relaxed), Relaxed);
        data.load(py, &source, "turtle", None, true).unwrap();
        let retained = LIVE.load(Relaxed);
        let peak = PEAK.load(Relaxed);
        println!("PARSER_HEAP peak_bytes={peak} retained_bytes={retained}");
        assert_eq!(data.__len__().unwrap(), 512);
        assert!(
            peak.saturating_sub(retained) < 4 * 1024 * 1024,
            "Parser retained a decoded lexical batch: peak={peak}, retained={retained}"
        );
    });
}

/// Late parse errors release inserted terms as well as restoring caller metadata.
#[test]
#[ignore = "global live counters require this test to run alone"]
fn failed_parse_releases_staged_terms() {
    let lexical = "x".repeat(64 * 1024);
    let mut source = String::from("@prefix ex: <urn:new:> .\n");
    for index in 0..128 {
        source.push_str(&format!(
            "_:s{index} ex:p \"{lexical}{index}\"^^<urn:datatype{index}> .\n"
        ));
    }
    source.push_str("invalid");
    Python::initialize();
    Python::attach(|py| {
        let data = NativeDataset::new();
        data.load(py, "@prefix ex: <urn:old:> . ex:s ex:p \"01\"^^<http://www.w3.org/2001/XMLSchema#integer> .", "turtle", None, true).unwrap();
        let original = data.dump(py, "nquads").unwrap();
        let namespaces = data.namespaces().unwrap();
        let before = LIVE.load(Relaxed);
        let error = data.load(py, &source, "turtle", None, true).unwrap_err();
        drop(error);
        let after = LIVE.load(Relaxed);
        println!("PARSER_ROLLBACK before_bytes={before} after_bytes={after}");
        assert!(
            after <= before + 256 * 1024,
            "Failed parse retained staged native allocations: before={before}, after={after}"
        );
        assert_eq!(data.dump(py, "nquads").unwrap(), original);
        assert_eq!(data.namespaces().unwrap(), namespaces);
        data.load(py, "<urn:next> <urn:p> <urn:o> .", "turtle", None, true)
            .unwrap();
        assert_eq!(data.__len__().unwrap(), 2);
    });
}

/// Six ordered indexes share each quad record instead of copying wide keys.
#[test]
#[ignore = "global live counters require this test to run alone"]
fn shared_index_retained_budget() {
    let baseline = LIVE.load(Relaxed);
    let mut data = oxrdf::Dataset::new();
    for index in 0..10_000 {
        data.insert(&Quad::new(
            NamedNode::new_unchecked(format!("urn:s{index}")),
            NamedNode::new_unchecked("urn:p"),
            Literal::new_simple_literal(format!("value{index}")),
            GraphName::DefaultGraph,
        ));
    }
    let retained = LIVE.load(Relaxed).saturating_sub(baseline);
    println!("INDEX_RETAINED bytes={retained} quads={}", data.len());
    assert_eq!(data.len(), 10_000);
    assert!(
        retained < 5_000_000,
        "Six-index retained allocation budget exceeded: {retained}"
    );
    std::hint::black_box(&data);
}

#[test]
#[ignore = "global allocation counters require isolated execution"]
fn scoped_query_binding_does_not_copy_graph_lexical() {
    let subject = NamedNode::new_unchecked("urn:s");
    let graph = NamedNode::new_unchecked(format!("urn:g{}", "x".repeat(64 * 1024)));
    let mut data = Dataset::new();
    data.insert(&Quad::new(
        subject.clone(),
        NamedNode::new_unchecked("urn:p"),
        Literal::new_simple_literal("value"),
        graph.clone(),
    ));
    let source = &data;
    let subject = spareval::QueryableDataset::internalize_term(&source, subject.into()).unwrap();
    let graph = spareval::QueryableDataset::internalize_term(&source, graph.into()).unwrap();
    let (rows, allocated) = measure_allocated(|| {
        spareval::QueryableDataset::internal_quads_for_pattern(
            &source,
            Some(&subject),
            None,
            None,
            Some(Some(&graph)),
        )
        .count()
    });
    println!("SCOPED_BINDING allocated_bytes={allocated} rows={rows}");
    assert_eq!(rows, 1);
    assert!(
        allocated < 8 * 1024,
        "Scoped query copied graph lexical storage: {allocated}"
    );
}
