//! The counting global allocator and its requested-byte measurements.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

use serde_json::json;

use super::current_thread;

struct CountingAllocator;
// Exact cache-ID payload requests for the isolated report regression (test-only).
pub(super) const REPORT_CACHE_PAYLOAD_SIZE: usize = 270;
pub(super) static REPORT_CACHE_PAYLOAD_LIVE: AtomicUsize = AtomicUsize::new(0);

pub(super) static LIVE: AtomicUsize = AtomicUsize::new(0);
pub(super) static PEAK: AtomicUsize = AtomicUsize::new(0);
pub(super) static TOTAL: AtomicUsize = AtomicUsize::new(0);
pub(super) static CALLS: AtomicUsize = AtomicUsize::new(0);

pub(super) fn allocated(size: usize) {
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

pub(super) fn phase(name: &str, quads: usize) {
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
