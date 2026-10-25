//! Explicit generalized storage over the existing shared six-index dataset core.
use super::*;

#[path = "dataset_generalized_collect.rs"]
mod collect;
#[path = "dataset_generalized_pattern.rs"]
mod pattern;
#[path = "dataset_generalized_remove.rs"]
mod remove;

/// An in-memory full-Term dataset, separate from ordinary strict RDF views.
///
/// It shares the same compact records, six indexes and interner implementation.
/// Like Dataset, committed dictionary words are not garbage collected.
#[derive(Debug, Default, Clone)]
pub struct GeneralizedDataset {
    pub(super) data: Dataset,
}
impl From<Dataset> for GeneralizedDataset {
    /// Moves the existing dictionary and six shared indexes without allocation.
    fn from(data: Dataset) -> Self {
        Self { data }
    }
}

impl GeneralizedDataset {
    /// Creates an empty generalized dataset.
    pub fn new() -> Self {
        Self::default()
    }
    /// Number of stored quads.
    pub fn len(&self) -> usize {
        self.data.len()
    }
    /// Whether no quads are stored.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    /// Inserts without restricting subject or predicate term kinds.
    pub fn insert<'a>(&mut self, quad: impl Into<GeneralizedQuadRef<'a>>) -> bool {
        let record = self.encode(quad.into());
        if self.data.gspo.contains(&record) {
            return false;
        }
        self.data.insert_record(Arc::new(record));
        true
    }
    /// Removes an exact generalized quad, preserving graph-name kinds.
    pub fn remove<'a>(&mut self, quad: impl Into<GeneralizedQuadRef<'a>>) -> bool {
        let Some(record) = self.encoded(quad.into()) else {
            return false;
        };
        self.data.remove_record(&record)
    }
    /// Tests exact generalized membership.
    pub fn contains<'a>(&self, quad: impl Into<GeneralizedQuadRef<'a>>) -> bool {
        self.encoded(quad.into())
            .is_some_and(|record| self.data.gspo.contains(&record))
    }
    /// Iterates every stored row, without narrowing or filtering term kinds.
    pub fn iter(&self) -> impl Iterator<Item = GeneralizedQuadRef<'_>> {
        self.data.spog.iter().map(|key| self.decode(&key.record))
    }
    /// Streams each named graph represented by a stored quad exactly once.
    /// Empty graph declarations are not represented by this in-memory owner.
    pub fn named_graphs(&self) -> impl Iterator<Item = NamedOrBlankNodeRef<'_>> {
        // bounded: one previous graph identity; GSPO groups all graph rows.
        let mut previous = None;
        self.data.gspo.iter().filter_map(move |key| {
            let graph = key.record.generalized_parts().3;
            if previous.replace(*graph) == Some(*graph) {
                return None;
            }
            match graph.decode_from(&self.data.interner) {
                GraphNameRef::NamedNode(node) => Some(node.into()),
                GraphNameRef::BlankNode(node) => Some(node.into()),
                GraphNameRef::DefaultGraph => None,
            }
        })
    }

    /// Retrieves generalized quads using the existing six ordered indexes.
    pub fn quads_for_pattern<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = GeneralizedQuadRef<'a>> + use<'a> {
        pattern::query(self, subject, predicate, object, graph)
    }
    /// Begins a change-only transaction supporting additions and removals.
    pub fn transaction(&mut self) -> GeneralizedTransaction<'_> {
        self.data.interner.begin_insertions();
        GeneralizedTransaction {
            data: self,
            journal: MutationJournal::default(),
            committed: false,
        }
    }
    fn encode(&mut self, q: GeneralizedQuadRef<'_>) -> QuadRecord {
        let interner = &mut self.data.interner;
        QuadRecord::from_generalized((
            InternedTerm::encoded_into(q.subject, interner),
            InternedTerm::encoded_into(q.predicate, interner),
            InternedTerm::encoded_into(q.object, interner),
            InternedGraphName::encoded_into(q.graph_name, interner),
        ))
    }
    fn encoded(&self, q: GeneralizedQuadRef<'_>) -> Option<QuadRecord> {
        let interner = &self.data.interner;
        Some(QuadRecord::from_generalized((
            InternedTerm::encoded_from(q.subject, interner)?,
            InternedTerm::encoded_from(q.predicate, interner)?,
            InternedTerm::encoded_from(q.object, interner)?,
            InternedGraphName::encoded_from(q.graph_name, interner)?,
        )))
    }
    pub(super) fn decode(&self, record: &QuadRecord) -> GeneralizedQuadRef<'_> {
        let (s, p, o, g) = record.generalized_parts();
        let i = &self.data.interner;
        GeneralizedQuadRef {
            subject: s.decode_from(i),
            predicate: p.decode_from(i),
            object: o.decode_from(i),
            graph_name: g.decode_from(i),
        }
    }
}

/// Generalized mutation guard retaining only changed facts, with unwind rollback.
pub struct GeneralizedTransaction<'a> {
    data: &'a mut GeneralizedDataset,
    pub(super) journal: MutationJournal,
    committed: bool,
}
impl GeneralizedTransaction<'_> {
    /// Inserts an absent full-Term quad.
    pub fn insert<'a>(&mut self, q: impl Into<GeneralizedQuadRef<'a>>) -> bool {
        let record = self.data.encode(q.into());
        if self.data.data.gspo.contains(&record) {
            return false;
        }
        let record = Arc::new(record);
        self.journal.inserted(record.clone());
        self.data.data.insert_record(record);
        true
    }
    /// Removes an exact full-Term quad and retains its original record.
    pub fn remove<'a>(&mut self, q: impl Into<GeneralizedQuadRef<'a>>) -> bool {
        let Some(record) = self.data.encoded(q.into()) else {
            return false;
        };
        let Some(record) = self.data.data.take_record(&record) else {
            return false;
        };
        self.journal.removed(record);
        true
    }
    /// Initially absent quads that are currently present, using current records.
    pub fn inserted(&self) -> impl Iterator<Item = GeneralizedQuadRef<'_>> {
        self.journal
            .current_insertions(&self.data.data)
            .map(|record| self.data.decode(record))
    }
    /// Retains all mutations.
    pub fn commit(mut self) {
        self.committed = true;
    }
}
impl std::ops::Deref for GeneralizedTransaction<'_> {
    type Target = GeneralizedDataset;
    fn deref(&self) -> &Self::Target {
        self.data
    }
}
impl Drop for GeneralizedTransaction<'_> {
    fn drop(&mut self) {
        if !self.committed {
            self.journal.rollback(&mut self.data.data);
        }
        self.data.data.interner.finish_insertions(self.committed);
    }
}
