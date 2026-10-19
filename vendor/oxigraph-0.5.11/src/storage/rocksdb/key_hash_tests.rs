use super::{DEFAULT_CF, Db, DbOptions, LATEST_STORAGE_VERSION, RocksDbStorage};
use tempfile::TempDir;

const KEY: &[u8] = b"turbo-strhash";
const SCHEME: &[u8] = b"siphash13-128";

#[test]
fn fresh_store_declares_its_lexical_hash_scheme() {
    let directory = TempDir::new().unwrap();
    let store = RocksDbStorage::open(directory.path()).unwrap();
    assert_eq!(
        store.db.get(&store.default_cf, KEY).unwrap().as_deref(),
        Some(SCHEME)
    );
    drop(store);
    let store = RocksDbStorage::open_read_only(directory.path()).unwrap();
    assert_eq!(
        store.db.get(&store.default_cf, KEY).unwrap().as_deref(),
        Some(SCHEME)
    );
}

#[test]
fn mismatched_hash_scheme_fails_for_writers_and_readers() {
    let directory = TempDir::new().unwrap();
    let store = RocksDbStorage::open(directory.path()).unwrap();
    store
        .db
        .insert(&store.default_cf, KEY, b"other-scheme")
        .unwrap();
    store.db.flush().unwrap();
    drop(store);
    for readonly in [false, true] {
        let result = if readonly {
            RocksDbStorage::open_read_only(directory.path())
        } else {
            RocksDbStorage::open(directory.path())
        };
        let error = result.err().expect("mismatched hash scheme was accepted");
        assert!(error.to_string().contains("lexical hash scheme"));
    }
}

#[test]
fn missing_hash_scheme_is_not_inferred_from_existing_data() {
    let directory = TempDir::new().unwrap();
    let db = Db::open_read_write(
        directory.path(),
        RocksDbStorage::column_families(),
        DbOptions::default(),
    )
    .unwrap();
    let default = db.column_family(DEFAULT_CF).unwrap();
    db.insert(
        &default,
        b"oxversion",
        &LATEST_STORAGE_VERSION.to_be_bytes(),
    )
    .unwrap();
    db.flush().unwrap();
    drop(db);
    for readonly in [false, true] {
        let result = if readonly {
            RocksDbStorage::open_read_only(directory.path())
        } else {
            RocksDbStorage::open(directory.path())
        };
        let error = result.err().expect("missing hash scheme was accepted");
        assert!(error.to_string().contains("lexical hash scheme"));
    }
    let db = Db::open_read_only(directory.path(), RocksDbStorage::column_families()).unwrap();
    let default = db.column_family(DEFAULT_CF).unwrap();
    assert!(db.get(&default, KEY).unwrap().is_none());
}

#[test]
fn hash_scheme_without_storage_version_is_rejected_without_repair() {
    let directory = TempDir::new().unwrap();
    let db = Db::open_read_write(
        directory.path(),
        RocksDbStorage::column_families(),
        DbOptions::default(),
    )
    .unwrap();
    let default = db.column_family(DEFAULT_CF).unwrap();
    db.insert(&default, KEY, SCHEME).unwrap();
    db.flush().unwrap();
    drop(db);
    let error = RocksDbStorage::open(directory.path())
        .err()
        .expect("incomplete hash scheme metadata was accepted");
    assert!(error.to_string().contains("lexical hash scheme"));
    let db = Db::open_read_only(directory.path(), RocksDbStorage::column_families()).unwrap();
    let default = db.column_family(DEFAULT_CF).unwrap();
    assert!(db.get(&default, b"oxversion").unwrap().is_none());
}

#[test]
fn undeclared_orphan_data_is_rejected_in_every_column_family() {
    // bounded: the fixed default and eleven declared column families.
    let mut names = vec![DEFAULT_CF];
    names.extend(
        RocksDbStorage::column_families()
            .iter()
            .map(|definition| definition.name),
    );
    for name in names {
        let directory = TempDir::new().unwrap();
        let db = Db::open_read_write(
            directory.path(),
            RocksDbStorage::column_families(),
            DbOptions::default(),
        )
        .unwrap();
        let column = db.column_family(name).unwrap();
        db.insert(&column, b"orphan", b"value").unwrap();
        db.flush().unwrap();
        drop(db);
        let error = RocksDbStorage::open(directory.path())
            .err()
            .expect("undeclared column data was accepted");
        assert!(error.to_string().contains("lexical hash scheme"));
    }
}
