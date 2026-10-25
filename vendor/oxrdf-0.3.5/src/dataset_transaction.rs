//! Change-only undo shared by strict and generalized dataset transactions.
use super::*;

#[derive(Debug)]
pub(super) enum MutationJournal {
    Additive(Vec<Arc<QuadRecord>>),
    Mutating {
        absent: HashSet<Arc<QuadRecord>>,
        present: HashSet<Arc<QuadRecord>>,
    },
}
impl Default for MutationJournal {
    fn default() -> Self {
        Self::Additive(Vec::new())
    }
}
impl MutationJournal {
    pub(super) fn inserted(&mut self, record: Arc<QuadRecord>) {
        match self {
            Self::Additive(records) => records.push(record),
            Self::Mutating { absent, present } => {
                if !present.contains(&record) {
                    absent.insert(record);
                }
            }
        }
    }
    pub(super) fn removed(&mut self, record: Arc<QuadRecord>) {
        if let Self::Additive(records) = self {
            let absent = take(records).into_iter().collect();
            *self = Self::Mutating {
                absent,
                present: HashSet::new(),
            };
        }
        let Self::Mutating { absent, present } = self else {
            unreachable!()
        };
        if !absent.contains(&record) {
            present.insert(record);
        }
    }
    pub(super) fn current_insertions<'a>(
        &'a self,
        data: &'a Dataset,
    ) -> impl Iterator<Item = &'a Arc<QuadRecord>> {
        let (additive, changed) = match self {
            Self::Additive(records) => (records.as_slice(), None),
            Self::Mutating { absent, .. } => (&[][..], Some(absent)),
        };
        // Additive entries are still live: retain direct O(delta) iteration.
        // Only changed keys require lookup of the current record incarnation.
        additive.iter().chain(
            changed
                .into_iter()
                .flatten()
                .filter_map(|record| data.gspo.get(record.as_ref()).map(|key| &key.record)),
        )
    }
    pub(super) fn rollback(&mut self, data: &mut Dataset) {
        match self {
            Self::Additive(records) => {
                for record in records.drain(..).rev() {
                    data.remove_record(&record);
                }
            }
            Self::Mutating { absent, present } => {
                for record in absent.drain() {
                    data.remove_record(&record);
                }
                for record in present.drain() {
                    data.remove_record(&record);
                    data.insert_record(record);
                }
            }
        }
    }
}

/// A transaction with change-only undo and no mutable dataset dereference.
pub struct InsertTransaction<'a> {
    pub(super) dataset: &'a mut Dataset,
    pub(super) journal: MutationJournal,
    pub(super) committed: bool,
}

impl InsertTransaction<'_> {
    /// Inserts only a previously absent quad, recording its compact interned key.
    pub fn insert<'a>(&mut self, quad: impl Into<QuadRef<'a>>) -> bool {
        let encoded = self.dataset.encode_quad(quad.into());
        let record = QuadRecord::from_spog(encoded);
        if self.dataset.gspo.contains(&record) {
            return false;
        }
        let record = Arc::new(record);
        self.journal.inserted(record.clone());
        self.dataset.insert_record(record);
        true
    }

    /// Quads inserted by this transaction, excluding preexisting or duplicate quads.
    pub fn inserted(&self) -> impl Iterator<Item = QuadRef<'_>> {
        self.journal
            .current_insertions(self.dataset)
            .map(|q| self.dataset.decode_spog(q.parts()))
    }

    /// Removes a quad and records its first-state original record for rollback.
    pub fn remove<'a>(&mut self, quad: impl Into<QuadRef<'a>>) -> bool {
        let Some(encoded) = self.dataset.encoded_quad(quad.into()) else {
            return false;
        };
        let Some(record) = self.dataset.take_record(&QuadRecord::from_spog(encoded)) else {
            return false;
        };
        self.journal.removed(record);
        true
    }

    /// Retains all mutations. Dropping the journal does not copy the dataset.
    pub fn commit(mut self) {
        self.committed = true;
    }
}

impl std::ops::Deref for InsertTransaction<'_> {
    type Target = Dataset;

    fn deref(&self) -> &Dataset {
        self.dataset
    }
}

impl Extend<Quad> for InsertTransaction<'_> {
    fn extend<T: IntoIterator<Item = Quad>>(&mut self, iter: T) {
        for quad in iter {
            self.insert(&quad);
        }
    }
}

impl Drop for InsertTransaction<'_> {
    fn drop(&mut self) {
        if !self.committed {
            self.journal.rollback(self.dataset);
        }
        self.dataset.interner.finish_insertions(self.committed);
    }
}
