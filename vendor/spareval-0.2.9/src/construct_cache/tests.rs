use super::ConstructCache;
use rustc_hash::FxHashSet;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct RequestedBytesAllocator;

thread_local! {
    static MEASURING: Cell<bool> = const { Cell::new(false) };
    static REQUESTED: Cell<usize> = const { Cell::new(0) };
}

// SAFETY: forwards the exact allocation and deallocation arguments to System.
// Only successful requested allocations on the measured test thread are counted.
#[expect(
    unsafe_code,
    reason = "test-only transparent System allocator accounting"
)]
unsafe impl GlobalAlloc for RequestedBytesAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplies the GlobalAlloc layout contract.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            let _ = MEASURING.try_with(|active| {
                if active.get() {
                    REQUESTED.with(|bytes| bytes.set(bytes.get() + layout.size()));
                }
            });
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: these are the original pointer and layout from the caller.
        unsafe { System.dealloc(pointer, layout) };
    }
}

#[global_allocator]
static ALLOCATOR: RequestedBytesAllocator = RequestedBytesAllocator;

fn requested_bytes(f: impl FnOnce()) -> usize {
    REQUESTED.with(|bytes| bytes.set(0));
    MEASURING.with(|active| assert!(!active.replace(true)));
    f();
    MEASURING.with(|active| active.set(false));
    REQUESTED.with(Cell::get)
}

#[test]
fn compact_result_identities_halve_actual_requested_key_storage() {
    // bounded: fixed 4096-result allocation fixture; this is not a timing or RSS test.
    let mut legacy = FxHashSet::<[usize; 3]>::default();
    let legacy_bytes = requested_bytes(|| {
        for identity in 0..4096 {
            assert!(legacy.insert([identity, identity + 1, identity + 2]));
        }
    });
    let mut cache = ConstructCache::default();
    let compact_bytes = requested_bytes(|| {
        for identity in 0..4096 {
            assert!(cache.insert([identity, identity + 1, identity + 2]));
        }
    });
    assert_eq!(cache.len(), legacy.len());
    let within_budget = if cfg!(target_pointer_width = "64") {
        compact_bytes * 5 <= legacy_bytes * 3
    } else {
        compact_bytes <= legacy_bytes
    };
    assert!(
        within_budget,
        "CONSTRUCT identity cache requests {compact_bytes}B versus legacy {legacy_bytes}B"
    );
    for identity in legacy {
        assert!(cache.contains(&identity));
    }
}

#[test]
fn construct_cache_preserves_all_positions_and_clear_reuse() {
    let mut cache = ConstructCache::default();
    let limit = usize::try_from(u32::MAX).unwrap();
    // bounded: fixed permutations distinguish subject, predicate and object identity.
    let keys = [[0, 1, limit], [0, limit, 1], [1, 0, limit], [limit, 0, 1]];
    for (index, key) in keys.iter().enumerate() {
        assert!(!cache.contains(key));
        assert!(cache.insert(*key));
        assert!(!cache.insert(*key));
        assert_eq!(cache.len(), index + 1);
    }
    cache.clear();
    assert_eq!(cache.len(), 0);
    for key in &keys {
        assert!(!cache.contains(key));
        assert!(cache.insert(*key));
    }
    assert_eq!(cache.len(), keys.len());
}

#[test]
#[cfg(target_pointer_width = "64")]
fn wide_result_identities_never_alias_small_keys_in_any_position() {
    let mut cache = ConstructCache::default();
    let limit = usize::try_from(u32::MAX).unwrap();
    let mut expected = FxHashSet::default();
    // bounded: boundary IDs plus their low-bit aliases in every statement position.
    for position in 0..3 {
        for identity in [0, 1, limit - 1, limit, limit + 1, limit + 2, usize::MAX] {
            let mut key = [7, 11, 13];
            key[position] = identity;
            assert_eq!(cache.insert(key), expected.insert(key));
            assert!(!cache.insert(key));
            assert_eq!(cache.len(), expected.len());
            for seen in &expected {
                assert!(cache.contains(seen));
            }
        }
    }
    let before = cache.len();
    let missing = [usize::MAX, usize::MAX, usize::MAX];
    assert!(!cache.contains(&missing));
    assert_eq!(cache.len(), before);
    cache.clear();
    assert_eq!(cache.len(), 0);
    for key in expected {
        assert!(!cache.contains(&key));
        assert!(cache.insert(key));
    }
}
