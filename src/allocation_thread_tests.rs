//! Allocation accounting for direct synchronous probes, excluding other threads.
use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

thread_local! {
    // Constant, destructor-free TLS does not allocate on first access.
    static LOCAL_TOTAL: Cell<Option<usize>> = const { Cell::new(None) };
}

pub(super) fn allocated(size: usize) {
    LOCAL_TOTAL.with(|counter| {
        if let Some(total) = counter.get() {
            match total.checked_add(size) {
                Some(next) => counter.set(Some(next)),
                None => std::process::abort(),
            }
        }
    });
}

struct Scope;

impl Drop for Scope {
    fn drop(&mut self) {
        LOCAL_TOTAL.with(|counter| counter.set(None));
    }
}

/// Count this synchronous caller only; never use for worker-dispatched operations.
pub(crate) fn measure_allocated<T>(operation: impl FnOnce() -> T) -> (T, usize) {
    LOCAL_TOTAL.with(|counter| {
        assert!(
            counter.get().is_none(),
            "nested current-thread allocation scope"
        );
        counter.set(Some(0));
    });
    let scope = Scope;
    let result = operation();
    let bytes = LOCAL_TOTAL.with(|counter| counter.get().expect("active allocation scope"));
    drop(scope);
    (result, bytes)
}

#[test]
fn direct_scope_excludes_foreign_thread_allocations() {
    let state = Arc::new(AtomicU8::new(0));
    let worker_state = Arc::clone(&state);
    let worker = std::thread::spawn(move || {
        worker_state.store(1, Ordering::Release);
        while worker_state.load(Ordering::Acquire) != 2 {
            std::hint::spin_loop();
        }
        let retained = std::hint::black_box(vec![7_u8; 144]);
        worker_state.store(3, Ordering::Release);
        while worker_state.load(Ordering::Acquire) != 4 {
            std::hint::spin_loop();
        }
        drop(retained);
    });
    while state.load(Ordering::Acquire) != 1 {
        std::hint::spin_loop();
    }
    let ((), allocated) = measure_allocated(|| {
        state.store(2, Ordering::Release);
        while state.load(Ordering::Acquire) != 3 {
            std::hint::spin_loop();
        }
    });
    state.store(4, Ordering::Release);
    worker.join().unwrap();
    assert_eq!(allocated, 0, "foreign allocation entered the direct scope");
}

#[test]
fn direct_scope_counts_small_caller_allocations() {
    let (retained, allocated) = measure_allocated(|| std::hint::black_box(vec![9_u8; 144]));
    assert_eq!(allocated, 144);
    drop(retained);
}

#[test]
fn direct_scope_counts_full_lexical_clones() {
    let text = "x".repeat(64 * 1024);
    let (retained, allocated) = measure_allocated(|| std::hint::black_box(text.clone()));
    assert_eq!(allocated, 64 * 1024);
    drop(retained);
}
