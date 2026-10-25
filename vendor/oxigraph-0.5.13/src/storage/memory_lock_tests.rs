//! First-touch allocation contract for the actual MVCC quad range lock.

use super::{EncodedQuad, EncodedTerm, QuadListNode, QuadRangeMutex, VersionRange};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct CountingAllocator;

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
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_allocation(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record_allocation(size);
        unsafe { System.realloc(pointer, layout, size) }
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
    layout!(EncodedGraphName);
    layout!(EncodedQuad);
    layout!(QuadListNode);
    layout!(VersionRange);
}
