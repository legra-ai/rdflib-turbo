//! First-touch allocation contract for the actual MVCC quad range lock.

use super::{EncodedQuad, EncodedTerm, QuadListNode, QuadRangeMutex, VersionRange};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

struct CountingAllocator;

// Test-only global live accounting includes allocations predating a measured scope.
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn observe_peak(live: usize) {
    let _ = TRACKING.try_with(|tracking| {
        if tracking.get() {
            PEAK.fetch_max(live, Relaxed);
        }
    });
}

thread_local! {
    static TRACKING: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
    static BYTES: Cell<usize> = const { Cell::new(0) };
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn record_allocation(bytes: usize) {
    let _ = TRACKING.try_with(|tracking| {
        if tracking.get() {
            ALLOCATIONS.with(|count| count.set(count.get() + 1));
            BYTES.with(|count| count.set(count.get() + bytes));
        }
    });
}

// SAFETY: this test-only wrapper forwards pointers/layouts unchanged to System;
// the const thread-local counters never allocate or alter allocator ownership.
#[allow(unsafe_code)]
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation(layout.size());
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            observe_peak(LIVE.fetch_add(layout.size(), Relaxed) + layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_allocation(layout.size());
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            observe_peak(LIVE.fetch_add(layout.size(), Relaxed) + layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Relaxed);
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record_allocation(size);
        let replacement = unsafe { System.realloc(pointer, layout, size) };
        // Failed realloc keeps the old allocation live; failed alloc adds nothing.
        // Existing TLS call/byte counters intentionally retain their request semantics.
        if !replacement.is_null() {
            let live = if size >= layout.size() {
                let growth = size - layout.size();
                LIVE.fetch_add(growth, Relaxed) + growth
            } else {
                let reduction = layout.size() - size;
                LIVE.fetch_sub(reduction, Relaxed) - reduction
            };
            observe_peak(live);
        }
        replacement
    }
}

pub(super) fn measure_allocations(operation: impl FnOnce()) -> (usize, usize) {
    struct StopTracking;
    impl Drop for StopTracking {
        fn drop(&mut self) {
            TRACKING.with(|tracking| tracking.set(false));
        }
    }
    assert!(
        !TRACKING.with(Cell::get),
        "Allocation measurements must not nest"
    );
    ALLOCATIONS.with(|count| count.set(0));
    BYTES.with(|count| count.set(0));
    TRACKING.with(|tracking| tracking.set(true));
    let guard = StopTracking;
    operation();
    drop(guard);
    (ALLOCATIONS.with(Cell::get), BYTES.with(Cell::get))
}

#[derive(Debug)]
struct RequestedHeap {
    live_before: usize,
    live_after: usize,
    peak: usize,
    calls: usize,
    allocated: usize,
}

fn measure_requested_heap(operation: impl FnOnce()) -> RequestedHeap {
    assert!(
        !TRACKING.with(Cell::get),
        "Requested heap measurements must not nest"
    );
    let live_before = LIVE.load(Relaxed);
    // Only the explicit isolated measurement entry resets highwater; live is never reset.
    PEAK.store(live_before, Relaxed);
    let (calls, allocated) = measure_allocations(operation);
    RequestedHeap {
        live_before,
        live_after: LIVE.load(Relaxed),
        peak: PEAK.load(Relaxed),
        calls,
        allocated,
    }
}

#[test]
#[ignore = "exact isolated allocator process required"]
fn requested_heap_warmed_control_allocates_nothing() {
    let empty = measure_requested_heap(|| std::hint::black_box(()));
    assert_eq!(empty.live_before, empty.live_after);
    assert_eq!(empty.peak, empty.live_before);
    assert_eq!((empty.calls, empty.allocated), (0, 0));
}

#[test]
#[ignore = "exact isolated allocator process required"]
fn requested_heap_tracks_zeroed_allocation_and_its_release() {
    let mut bytes = None;
    let allocation = measure_requested_heap(|| {
        bytes = Some(vec![0u8; 4096]);
        std::hint::black_box(&bytes);
    });
    assert_eq!(allocation.live_after, allocation.live_before + 4096);
    assert!(allocation.peak >= allocation.live_after);
    assert_eq!((allocation.calls, allocation.allocated), (1, 4096));
    let release = measure_requested_heap(|| drop(bytes.take()));
    assert_eq!(release.live_before, allocation.live_after);
    assert_eq!(release.live_after, allocation.live_before);
    assert_eq!((release.calls, release.allocated), (0, 0));
}

#[test]
#[ignore = "exact isolated allocator process required"]
fn requested_heap_tracks_retained_allocation_and_reallocation() {
    let mut bytes = Vec::with_capacity(16);
    bytes.push(1u8);
    let original_capacity = bytes.capacity();
    let allocation = measure_requested_heap(|| {
        bytes.reserve_exact(8192);
        std::hint::black_box(&bytes);
    });
    // reserve_exact takes additional elements relative to length, not existing capacity.
    assert_eq!(
        allocation.live_after,
        allocation.live_before + bytes.capacity() - original_capacity
    );
    assert!(allocation.peak >= allocation.live_after);
    assert_eq!(
        (allocation.calls, allocation.allocated),
        (1, bytes.capacity())
    );
    drop(bytes);
}

#[test]
#[ignore = "exact isolated allocator process required"]
fn requested_heap_can_release_a_preexisting_allocation_without_underflow() {
    let bytes: Vec<u8> = Vec::with_capacity(4096);
    let allocation = measure_requested_heap(|| drop(bytes));
    assert!(allocation.live_before >= 4096);
    assert_eq!(allocation.live_after, allocation.live_before - 4096);
    assert_eq!((allocation.calls, allocation.allocated), (0, 0));
    assert_eq!(allocation.peak, allocation.live_before);
}

#[test]
fn quad_range_first_lock_does_not_allocate() {
    // bounded: 32 independent first touches expose per-quad backing allocations.
    let nodes: Vec<_> = (0..32)
        .map(|_| QuadListNode {
            quad: EncodedQuad {
                subject: EncodedTerm::DefaultGraph,
                predicate: EncodedTerm::DefaultGraph,
                object: EncodedTerm::DefaultGraph,
                graph_name: EncodedTerm::DefaultGraph.try_into().unwrap(),
            },
            range: QuadRangeMutex::new(VersionRange::Start(1)),
            previous: None,
            previous_subject: None,
            previous_predicate: None,
            previous_object: None,
            previous_graph_name: None,
        })
        .collect();
    ALLOCATIONS.with(|count| count.set(0));
    TRACKING.with(|tracking| tracking.set(true));
    for node in &nodes {
        let guard = node.range.lock();
        std::hint::black_box(&guard);
        drop(guard);
    }
    TRACKING.with(|tracking| tracking.set(false));
    let allocations = ALLOCATIONS.with(Cell::get);
    assert_eq!(
        allocations, 0,
        "quad range first-touch allocated backing storage"
    );
}

#[test]
#[ignore = "explicit production-type layout observation; no allocator-bin or RSS claim"]
fn profile_actual_storage_type_layouts() {
    use super::super::numeric_encoder::EncodedGraphName;
    use std::mem::{align_of, size_of};
    macro_rules! layout {
        ($ty:ty) => {
            println!(
                "MEMORY_LAYOUT {{\"type\":\"{}\",\"size\":{},\"align\":{}}}",
                stringify!($ty),
                size_of::<$ty>(),
                align_of::<$ty>()
            );
        };
    }
    layout!(EncodedTerm);
    layout!(spareval::InternalQuad<EncodedTerm>);
    layout!((EncodedTerm, EncodedTerm, EncodedTerm));
    layout!((EncodedTerm, usize));
    layout!([usize; 3]);
    layout!(EncodedGraphName);
    layout!(EncodedQuad);
    layout!(QuadListNode);
    layout!(VersionRange);
}

#[cfg(feature = "exact-term-storage")]
#[path = "memory_union_cache_tests.rs"]
mod union_cache;
