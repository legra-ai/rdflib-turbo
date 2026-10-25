//! Optional first-presence capture over actual MVCC row mutations.
use super::{MemoryStorageReader, MemoryStorageTransaction, QuadListNode, StorageError};
use crate::storage::numeric_encoder::EncodedQuad;
use std::collections::HashSet;
use std::io;
use std::sync::Arc;

pub(super) enum Capture {
    Additive(Vec<Arc<QuadListNode>>),
    Prepared,
    Mutating {
        absent: HashSet<Arc<QuadListNode>>,
        present: HashSet<Arc<QuadListNode>>,
    },
}

impl Capture {
    fn inserted(&mut self, row: &Arc<QuadListNode>) {
        match self {
            Self::Prepared => panic!("Prepared capture cannot mutate"),
            Self::Additive(rows) => rows.push(Arc::clone(row)),
            Self::Mutating { absent, present } => {
                if !present.contains(row) {
                    absent.insert(Arc::clone(row));
                }
            }
        }
    }

    fn removed(&mut self, row: &Arc<QuadListNode>) {
        assert!(
            !matches!(self, Self::Prepared),
            "Prepared capture cannot mutate"
        );
        if let Self::Additive(rows) = self {
            *self = Self::Mutating {
                absent: std::mem::take(rows).into_iter().collect(),
                present: HashSet::new(),
            };
        }
        let Self::Mutating { absent, present } = self else {
            unreachable!()
        };
        if !absent.contains(row) {
            present.insert(Arc::clone(row));
        }
    }

    fn finish(self, transaction_id: usize) -> (Vec<Arc<QuadListNode>>, Vec<Arc<QuadListNode>>) {
        match self {
            Self::Prepared => panic!("Prepared capture was already finalized"),
            // No actual removals occurred, so every captured addition remains live.
            Self::Additive(rows) => (rows, Vec::new()),
            Self::Mutating { absent, present } => (
                absent
                    .into_iter()
                    .filter(|row| row.range.lock().contains(transaction_id))
                    .collect(),
                present
                    .into_iter()
                    .filter(|row| !row.range.lock().contains(transaction_id))
                    .collect(),
            ),
        }
    }
}

/// Encapsulated committed row identities and the owning dictionary/version lease.
pub(crate) struct MemoryChanges {
    pub(super) reader: MemoryStorageReader<'static>,
    pub(super) added: Vec<Arc<QuadListNode>>,
    pub(super) removed: Vec<Arc<QuadListNode>>,
}

pub(crate) struct MemoryChangedRows(std::vec::IntoIter<Arc<QuadListNode>>);

impl Iterator for MemoryChangedRows {
    type Item = EncodedQuad;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|row| row.quad.clone())
    }
}

impl MemoryChanges {
    pub(crate) fn into_parts(
        self,
    ) -> (
        MemoryStorageReader<'static>,
        MemoryChangedRows,
        MemoryChangedRows,
    ) {
        (
            self.reader,
            MemoryChangedRows(self.added.into_iter()),
            MemoryChangedRows(self.removed.into_iter()),
        )
    }
}

/// Finalized changed identities; decoding remains tied to the pending writer.
pub(crate) struct MemoryPreparedChanges {
    owner: super::MemoryStorage,
    transaction_id: usize,
    added: std::vec::IntoIter<Arc<QuadListNode>>,
    removed: std::vec::IntoIter<Arc<QuadListNode>>,
}

impl MemoryPreparedChanges {
    pub(crate) fn next(&mut self, added: bool) -> Option<EncodedQuad> {
        let rows = if added {
            &mut self.added
        } else {
            &mut self.removed
        };
        rows.next().map(|row| row.quad.clone())
    }

    pub(crate) fn discard_removals(&mut self) {
        self.removed.by_ref().for_each(drop);
    }

    pub(crate) fn stream_finished(&self, added: bool) -> bool {
        if added {
            self.added.len() == 0
        } else {
            self.removed.len() == 0
        }
    }

    pub(crate) fn finished(&self) -> bool {
        self.added.len() == 0 && self.removed.len() == 0
    }
}

impl MemoryStorageTransaction<'_> {
    pub fn prepare_changes(&mut self) -> Result<MemoryPreparedChanges, StorageError> {
        if self.capture.is_none() || matches!(self.capture.as_deref(), Some(Capture::Prepared)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Transaction has no unprepared change capture",
            )
            .into());
        }
        let capture = self.capture.take().expect("checked active capture");
        let (added, removed) = capture.finish(self.transaction_id);
        self.capture = Some(Box::new(Capture::Prepared));
        Ok(MemoryPreparedChanges {
            owner: self.storage.clone(),
            transaction_id: self.transaction_id,
            added: added.into_iter(),
            removed: removed.into_iter(),
        })
    }

    pub fn validate_prepared(&self, changes: &MemoryPreparedChanges) -> Result<(), StorageError> {
        if !matches!(self.capture.as_deref(), Some(Capture::Prepared))
            || self.transaction_id != changes.transaction_id
            || !Arc::ptr_eq(&self.storage.id2str, &changes.owner.id2str)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Prepared changes belong to a different transaction",
            )
            .into());
        }
        Ok(())
    }

    pub fn commit_prepared_changes(
        mut self,
        changes: MemoryPreparedChanges,
    ) -> Result<(), StorageError> {
        self.validate_prepared(&changes)?;
        if !changes.finished() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Prepared changes must be consumed before acceptance",
            )
            .into());
        }
        drop(changes);
        self.capture.take();
        self.publish();
        Ok(())
    }

    pub fn ensure_no_capture(&self) -> Result<(), StorageError> {
        if self.capture.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Active change capture requires commit_with_changes",
            )
            .into());
        }
        Ok(())
    }

    pub fn begin_changes(&mut self) -> Result<(), StorageError> {
        if self.capture.is_some() || !self.log.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Change capture requires a new transaction without pending writes",
            )
            .into());
        }
        self.capture = Some(Box::new(Capture::Additive(Vec::new())));
        Ok(())
    }

    pub(super) fn capture_insert(&mut self, row: &Arc<QuadListNode>) {
        if let Some(capture) = &mut self.capture {
            capture.inserted(row);
        }
    }

    pub(super) fn capture_remove(&mut self, row: &Arc<QuadListNode>) {
        if let Some(capture) = &mut self.capture {
            capture.removed(row);
        }
    }

    pub fn commit_with_changes(mut self) -> Result<MemoryChanges, StorageError> {
        if matches!(self.capture.as_deref(), Some(Capture::Prepared)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Prepared changes require explicit acceptance",
            )
            .into());
        }
        let capture = self.capture.take().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Transaction has no active change capture",
            )
        })?;
        let (added, removed) = capture.finish(self.transaction_id);
        self.publish();
        // self still owns _transaction_guard: construct the owning published
        // reader before another writer can mutate the relation or its dictionary.
        let reader = self.storage.snapshot();
        Ok(MemoryChanges {
            reader,
            added,
            removed,
        })
    }
}
