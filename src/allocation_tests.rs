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
