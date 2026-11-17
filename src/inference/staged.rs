//! Infer through the existing insertion journal and a borrowed logical union.
#[cfg(all(test, feature = "allocation-profiling"))]
#[path = "staged_allocation_tests.rs"]
mod allocation_tests;
#[path = "staged_dataset.rs"]
mod dataset;
#[path = "staged_journal.rs"]
mod journal;
#[path = "staged_query.rs"]
mod query;

pub(super) use journal::{JournalGraph, expand};
