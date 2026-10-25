//! Generalized data operations over the existing encoded relation and MVCC owner.

use super::{StorageError, Store, Transaction};
use crate::model::{GeneralizedQuad, GeneralizedQuadRef, GraphNameRef, TermRef};
use crate::storage::numeric_encoder::{Decoder, EncodedQuad, EncodedTerm};
use crate::storage::{CorruptionError, DecodingQuadIterator, StorageReader};
use std::sync::Arc;

#[path = "generalized_lexical.rs"]
mod lexical;
pub use lexical::GeneralizedLexicalQuadIter;
#[path = "generalized_quad_lexical.rs"]
mod quad_lexical;
pub use quad_lexical::{
    GeneralizedLexicalGraphRows, GeneralizedLexicalQuadRows, GeneralizedLexicalUnionRows,
    LexicalGraphName, LexicalQuad, LexicalUnionRow,
};

#[cfg(test)]
#[path = "generalized_changes_tests.rs"]
mod changes_tests;

/// Committed generalized changes retaining one owning storage/dictionary lease.
#[doc(hidden)]
pub struct CommittedChanges {
    additions: GeneralizedQuadIter<'static>,
    removals: GeneralizedQuadIter<'static>,
}

impl CommittedChanges {
    /// Consume the changed-only additions and removals, without a second relation.
    pub fn into_parts(self) -> (GeneralizedQuadIter<'static>, GeneralizedQuadIter<'static>) {
        (self.additions, self.removals)
    }
}

/// Finalized pending changed identities. Its transaction must remain unpublished.
#[doc(hidden)]
pub struct PreparedChanges {
    inner: crate::storage::PreparedChanges,
}

impl PreparedChanges {
    /// Explicitly consume removals without decoding or restoring their terms.
    #[doc(hidden)]
    pub fn discard_removals(&mut self) {
        self.inner.discard_removals();
    }
}

impl Transaction<'_> {
    /// Finalize changed identities without publishing the pending transaction.
    #[doc(hidden)]
    pub fn prepare_memory_changes(&mut self) -> Result<PreparedChanges, StorageError> {
        self.inner_mut()
            .prepare_memory_changes()
            .map(|inner| PreparedChanges { inner })
    }

    /// Decode one bounded batch under this pending writer; no reader escapes.
    #[doc(hidden)]
    pub fn visit_prepared_changes(
        &self,
        changes: &mut PreparedChanges,
        added: bool,
        mut visit: impl FnMut(LexicalQuad) -> bool,
    ) -> Result<bool, StorageError> {
        let reader = self.inner.prepared_reader(&changes.inner)?;
        while let Some(quad) = changes.inner.next(added) {
            if !visit(LexicalQuad::decode(&reader, quad)?) {
                break;
            }
        }
        Ok(changes.inner.stream_finished(added))
    }

    /// Accept only completely consumed prepared changes on their original writer.
    #[doc(hidden)]
    pub fn commit_prepared_changes(self, changes: PreparedChanges) -> Result<(), StorageError> {
        self.inner.commit_prepared_changes(changes.inner)
    }

    /// Begin private changed-only capture on a new in-memory transaction.
    #[doc(hidden)]
    pub fn begin_memory_changes(&mut self) -> Result<(), StorageError> {
        self.inner_mut().begin_memory_changes()
    }

    /// Publish and pin the committed changes before releasing the writer permit.
    #[doc(hidden)]
    pub fn commit_with_changes(self) -> Result<CommittedChanges, StorageError> {
        let (reader, added, removed) = self.inner.commit_with_changes()?;
        let reader = Arc::new(reader);
        Ok(CommittedChanges {
            additions: GeneralizedQuadIter {
                iter: added,
                unique_spo: false,
                fixed_graph: false,
                reader: Arc::clone(&reader),
            },
            removals: GeneralizedQuadIter {
                iter: removed,
                unique_spo: false,
                fixed_graph: false,
                reader,
            },
        })
    }
}

/// A lazy generalized cursor retaining the actual pinned storage and dictionary.
#[must_use]
pub struct GeneralizedQuadIter<'a> {
    iter: DecodingQuadIterator<'a>,
    reader: Arc<StorageReader<'a>>,
    unique_spo: bool,
    fixed_graph: bool,
}

impl GeneralizedQuadIter<'static> {
    /// Return union statements with lazy physical contexts from this same reader.
    #[doc(hidden)]
    pub fn project_union_lexical(self) -> GeneralizedLexicalUnionRows {
        GeneralizedLexicalUnionRows::new(self)
    }
}

impl<'a> GeneralizedQuadIter<'a> {
    /// Select one physical representative for each exact encoded SPO.
    /// The same pinned reader performs both the scan and indexed representative
    /// lookup; this retains no seen-set and decodes no rejected lexical payloads.
    #[doc(hidden)]
    pub fn unique_spo(mut self) -> Self {
        // A fixed physical graph already contains each SPO at most once.
        // A global representative could live outside the selected graph.
        self.unique_spo = !self.fixed_graph;
        self
    }

    fn next_encoded(&mut self) -> Option<Result<EncodedQuad, StorageError>> {
        loop {
            let quad = match self.iter.next()? {
                Ok(quad) => quad,
                Err(error) => return Some(Err(error)),
            };
            if self.unique_spo {
                match self
                    .reader
                    .quads_for_pattern(
                        Some(&quad.subject),
                        Some(&quad.predicate),
                        Some(&quad.object),
                        None,
                    )
                    .next()
                {
                    Some(Ok(first)) if first.graph_name == quad.graph_name => {}
                    Some(Ok(_)) => continue,
                    Some(Err(error)) => return Some(Err(error)),
                    None => {
                        return Some(Err(CorruptionError::new(
                            "Pinned union representative is missing",
                        )
                        .into()));
                    }
                }
            }
            return Some(Ok(quad));
        }
    }

    /// Decode graph metadata only, retaining the same owning lexical reader.
    #[doc(hidden)]
    pub fn project_graph_lexical(self) -> GeneralizedLexicalGraphRows<'a> {
        GeneralizedLexicalGraphRows::new(self)
    }

    /// Project all full-Term statement fields and typed graph metadata.
    pub fn project_quad_lexical(self) -> GeneralizedLexicalQuadRows<'a> {
        GeneralizedLexicalQuadRows::new(self)
    }

    /// Select full-Term lexical fields without materializing RDF model strings.
    ///
    /// This explicit generalized capability preserves literal subjects and blank
    /// or literal predicates; ordinary QuadIter projection stays strict.
    pub fn project_lexical(
        self,
        subject: bool,
        predicate: bool,
        object: bool,
    ) -> GeneralizedLexicalQuadIter<'a> {
        GeneralizedLexicalQuadIter::new(self, [subject, predicate, object])
    }

    pub(super) fn new(
        reader: Arc<StorageReader<'a>>,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> Self {
        let iter = reader.quads_for_pattern(
            subject.map(EncodedTerm::from).as_ref(),
            predicate.map(EncodedTerm::from).as_ref(),
            object.map(EncodedTerm::from).as_ref(),
            graph.map(EncodedTerm::from).as_ref(),
        );
        Self {
            iter,
            reader,
            unique_spo: false,
            fixed_graph: graph.is_some(),
        }
    }
}

impl Iterator for GeneralizedQuadIter<'_> {
    type Item = Result<GeneralizedQuad, StorageError>;

    fn next(&mut self) -> Option<Self::Item> {
        Some(
            self.next_encoded()?
                .and_then(|quad| self.reader.decode_generalized_quad(&quad)),
        )
    }
}

impl Store {
    /// Tests exact generalized membership without decoding stored terms.
    pub fn contains_generalized<'a>(
        &self,
        quad: impl Into<GeneralizedQuadRef<'a>>,
    ) -> Result<bool, StorageError> {
        self.storage
            .snapshot()
            .contains(&EncodedQuad::from(quad.into()))
    }

    /// Insert generalized terms atomically into the existing encoded relation.
    pub fn insert_generalized<'a>(
        &self,
        quad: impl Into<GeneralizedQuadRef<'a>>,
    ) -> Result<(), StorageError> {
        let mut transaction = self.storage.start_transaction()?;
        transaction.insert(quad.into());
        transaction.commit()?;
        Ok(())
    }

    /// Remove a generalized statement atomically from the same relation.
    pub fn remove_generalized<'a>(
        &self,
        quad: impl Into<GeneralizedQuadRef<'a>>,
    ) -> Result<(), StorageError> {
        let mut transaction = self.storage.start_transaction()?;
        transaction.remove(quad.into());
        transaction.commit()?;
        Ok(())
    }

    /// Stream matching generalized rows from one owned committed version.
    pub fn generalized_quads_for_pattern(
        &self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> GeneralizedQuadIter<'static> {
        GeneralizedQuadIter::new(
            Arc::new(self.storage.snapshot()),
            subject,
            predicate,
            object,
            graph,
        )
    }
}

impl Transaction<'_> {
    /// Tests exact generalized membership on this transaction's pending view.
    pub fn contains_generalized<'a>(
        &self,
        quad: impl Into<GeneralizedQuadRef<'a>>,
    ) -> Result<bool, StorageError> {
        self.inner
            .reader()
            .contains(&EncodedQuad::from(quad.into()))
    }

    /// Insert generalized terms on the current transaction owner.
    pub fn insert_generalized<'a>(&mut self, quad: impl Into<GeneralizedQuadRef<'a>>) {
        self.inner.insert(quad.into());
    }

    /// Remove generalized terms on the current transaction owner.
    pub fn remove_generalized<'a>(&mut self, quad: impl Into<GeneralizedQuadRef<'a>>) {
        self.inner.remove(quad.into());
    }

    /// Stream pending rows without reacquiring another owner or committed snapshot.
    pub fn generalized_quads_for_pattern(
        &self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> GeneralizedQuadIter<'_> {
        GeneralizedQuadIter::new(
            Arc::new(self.inner.reader()),
            subject,
            predicate,
            object,
            graph,
        )
    }
}

#[cfg(test)]
#[path = "generalized_lexical_tests.rs"]
mod lexical_tests;
