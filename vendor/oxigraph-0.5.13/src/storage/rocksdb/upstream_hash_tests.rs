use super::{DEFAULT_CF, Db, DbOptions, LATEST_STORAGE_VERSION, RocksDbStorage};
use tempfile::TempDir;

#[test]
fn exact_hash_declarations_are_rejected_before_upstream_migration() {
    // bounded: both known and unknown declarations, with and without a version.
    for scheme in [b"siphash13-128".as_slice(), b"unknown".as_slice()] {
        for version in [None, Some(LATEST_STORAGE_VERSION)] {
            let directory = TempDir::new().unwrap();
            let db = Db::open_read_write(
                directory.path(),
                RocksDbStorage::column_families(),
                DbOptions::default(),
            )
            .unwrap();
            let default = db.column_family(DEFAULT_CF).unwrap();
            db.insert(&default, b"turbo-strhash", scheme).unwrap();
            if let Some(version) = version {
                db.insert(&default, b"oxversion", &version.to_be_bytes())
                    .unwrap();
            }
            db.flush().unwrap();
            drop(db);
            for readonly in [false, true] {
                let result = if readonly {
                    RocksDbStorage::open_read_only(directory.path())
                } else {
                    RocksDbStorage::open(directory.path())
                };
                let error = result
                    .err()
                    .expect("exact hash store was accepted upstream");
                assert!(error.to_string().contains("lexical hash scheme"));
            }
            let db =
                Db::open_read_only(directory.path(), RocksDbStorage::column_families()).unwrap();
            let default = db.column_family(DEFAULT_CF).unwrap();
            assert_eq!(
                db.get(&default, b"turbo-strhash").unwrap().as_deref(),
                Some(scheme)
            );
            let expected = version.map(u64::to_be_bytes);
            assert_eq!(
                db.get(&default, b"oxversion").unwrap().as_deref(),
                expected.as_ref().map(|v| v.as_slice())
            );
        }
    }
}
