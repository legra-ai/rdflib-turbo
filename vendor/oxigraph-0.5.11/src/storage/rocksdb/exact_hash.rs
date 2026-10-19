//! Exact stores declare their lexical key scheme before any indexed operation.
use super::{CorruptionError, LATEST_STORAGE_VERSION, RocksDbStorage, StorageError};

const HASH_KEY: &[u8] = b"turbo-strhash";
const HASH_SCHEME: &[u8] = b"siphash13-128";

impl RocksDbStorage {
    pub(super) fn ensure_exact_hash(&self) -> Result<(), StorageError> {
        if let Some(scheme) = self.db.get(&self.default_cf, HASH_KEY)? {
            if scheme.as_ref() != HASH_SCHEME {
                return Err(CorruptionError::msg("Unsupported lexical hash scheme").into());
            }
            let version = self.db.get(&self.default_cf, b"oxversion")?;
            if version.as_deref() != Some(LATEST_STORAGE_VERSION.to_be_bytes().as_slice()) {
                return Err(CorruptionError::msg(
                    "Storage version does not match the declared lexical hash scheme",
                )
                .into());
            }
            return Ok(());
        }
        let reader = self.db.snapshot();
        // bounded: exactly the twelve fixed column families of this store schema.
        for column in [
            &self.default_cf,
            &self.id2str_cf,
            &self.spog_cf,
            &self.posg_cf,
            &self.ospg_cf,
            &self.gspo_cf,
            &self.gpos_cf,
            &self.gosp_cf,
            &self.dspo_cf,
            &self.dpos_cf,
            &self.dosp_cf,
            &self.graphs_cf,
        ] {
            if !reader.is_empty(column)? {
                return Err(CorruptionError::msg(
                    "Existing store is missing its lexical hash scheme",
                )
                .into());
            }
        }
        drop(reader);
        if !self.db.is_writable() {
            return Err(
                CorruptionError::msg("Read-only store is missing its lexical hash scheme").into(),
            );
        }
        let mut transaction = self.db.start_transaction()?;
        transaction.insert(&self.default_cf, HASH_KEY, HASH_SCHEME);
        transaction.insert(
            &self.default_cf,
            b"oxversion",
            &LATEST_STORAGE_VERSION.to_be_bytes(),
        );
        transaction.commit()?;
        self.db.flush()
    }
}
