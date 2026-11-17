//! Storage failures must not publish partial round results or retain journal writes.
#[path = "failure_rollback_tests.rs"]
mod rollback;
#[path = "failure_storage_tests.rs"]
mod storage;
