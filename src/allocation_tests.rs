//! Requested Rust heap bytes, not allocator arenas, Python allocations or RSS.
//! Run this ignored test alone, with --test-threads=1. Production has no counter.
#[path = "allocation_budget_tests.rs"]
mod budgets;
#[path = "allocation_canonical_profile_tests.rs"]
mod canonical;
#[path = "allocation_counter_tests.rs"]
mod counter;
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

use counter::{
    CALLS, LIVE, PEAK, REPORT_CACHE_PAYLOAD_LIVE, REPORT_CACHE_PAYLOAD_SIZE, TOTAL, phase,
};
pub(crate) use counter::{
    RequestedAllocation, live_requested_bytes, measure_allocated, measure_requested,
};
