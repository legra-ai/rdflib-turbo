use super::{MemoryStorage, StrHash, StrLookup};
use crate::storage::{StorageReader, StorageReaderKind};
use std::sync::Arc;

#[test]
fn dictionary_lease_releases_guard_and_reports_missing_values() {
    let storage = MemoryStorage::new();
    let key = StrHash::new("dictionary-value-long-enough-to-be-shared");
    storage
        .id2str
        .insert(key, Arc::from("dictionary-value-long-enough-to-be-shared"));
    let reader = storage.snapshot();
    let lease = reader.get_shared_str(&key).unwrap();
    let copy = reader.get_str(&key).unwrap().unwrap();
    assert_eq!(lease.as_ref(), copy);
    assert_ne!(lease.as_ptr(), copy.as_ptr());
    // Removing a dictionary entry would deadlock if a lease retained its read guard.
    storage.id2str.remove(&key);
    assert_eq!(lease.as_ref(), copy);
    assert!(reader.get_shared_str(&key).is_none());
    let reader = StorageReader {
        kind: StorageReaderKind::Memory(reader),
    };
    assert!(reader.get_term_text(&key).is_err());
}
