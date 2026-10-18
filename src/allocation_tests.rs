//! Requested Rust heap bytes, not allocator arenas, Python allocations or RSS.
//! Run this ignored test alone, with --test-threads=1. Production has no counter.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

use super::*;

struct CountingAllocator;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static TOTAL: AtomicUsize = AtomicUsize::new(0);
static CALLS: AtomicUsize = AtomicUsize::new(0);

fn allocated(size: usize) {
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
        LIVE.fetch_sub(layout.size(), Relaxed);
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let next = unsafe { System.realloc(pointer, layout, size) };
        if !next.is_null() {
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

/// Measure requested bytes for one synchronous operation in an isolated test.
pub(crate) fn measure_allocated<T>(operation: impl FnOnce() -> T) -> (T, usize) {
    TOTAL.store(0, Relaxed);
    let result = operation();
    (result, TOTAL.load(Relaxed))
}

#[test]
#[ignore = "explicit release-mode profiling, run alone with a fixture and shapes file"]
fn profile_pipeline_allocations() {
    let source = std::fs::read_to_string(std::env::var("MEMORY_DATA").unwrap()).unwrap();
    let shapes = std::fs::read_to_string(std::env::var("MEMORY_SHAPES").unwrap()).unwrap();
    let mode = std::env::var("MEMORY_INFERENCE").unwrap();
    Python::initialize();
    Python::attach(|py| {
        let data = NativeDataset::new();
        phase("setup", 0);
        data.load(py, &source, "turtle", None, true).unwrap();
        phase("parse", data.__len__().unwrap());
        data.infer(py, &mode, None).unwrap();
        phase("inference", data.__len__().unwrap());
        let program = shacl::SharedShapes::new(py, &shapes, false, Vec::new(), false).unwrap();
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
        data.infer(py, "owlrl", None).unwrap();
        let peak = PEAK.load(Relaxed);
        let retained = LIVE.load(Relaxed);
        assert!(
            peak < retained * 2,
            "Inference heap budget exceeded: peak={peak}, retained={retained}"
        );
        let first = data.dump(py, "nquads").unwrap();
        data.infer(py, "owlrl", None).unwrap();
        assert_eq!(data.dump(py, "nquads").unwrap(), first);
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
