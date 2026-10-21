//! A pinned committed MVCC version, without acquiring a writer.
use super::{Store, StorageError};
use crate::model::*;
use crate::storage::StorageReader;
use crate::storage::numeric_encoder::{Decoder, EncodedTerm};

/// A consistent committed read view. Keeps the storage version alive without
/// copying quads or blocking a pending write transaction.
pub struct ReadSnapshot {
    reader: StorageReader<'static>,
}

impl Store {
    /// Pin the current committed version for indexed reads without a writer lock.
    pub fn read_snapshot(&self) -> ReadSnapshot {
        ReadSnapshot {
            reader: self.storage.snapshot(),
        }
    }
}

impl ReadSnapshot {
    /// Stream matching quads from the pinned version. None matches every graph.
    pub fn quads_for_pattern<'a>(
        &'a self,
        subject: Option<NamedOrBlankNodeRef<'_>>,
        predicate: Option<NamedNodeRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = Result<Quad, StorageError>> + use<'a> {
        self.reader
            .quads_for_pattern(
                subject.map(EncodedTerm::from).as_ref(),
                predicate.map(EncodedTerm::from).as_ref(),
                object.map(EncodedTerm::from).as_ref(),
                graph.map(EncodedTerm::from).as_ref(),
            )
            .map(|quad| self.reader.decode_quad(&quad?))
    }

    /// Stream named graphs, including empty graphs, from the pinned version.
    pub fn named_graphs(
        &self,
    ) -> impl Iterator<Item = Result<NamedOrBlankNode, StorageError>> + '_ {
        self.reader
            .named_graphs()
            .map(|graph| self.reader.decode_named_or_blank_node(&graph?))
    }

    /// Test named graph existence in the pinned version.
    pub fn contains_named_graph(
        &self,
        graph: NamedOrBlankNodeRef<'_>,
    ) -> Result<bool, StorageError> {
        self.reader.contains_named_graph(&EncodedTerm::from(graph))
    }
}
