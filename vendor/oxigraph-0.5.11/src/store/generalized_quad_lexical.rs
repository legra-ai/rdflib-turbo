//! Full-Term quad projection with a separate typed graph-name domain.
use super::super::{LexicalTerm, StorageError, TermText};
use super::GeneralizedQuadIter;
use crate::model::BlankNode;
use crate::storage::DecodingQuadIterator;
use crate::storage::StorageReader;
use crate::storage::numeric_encoder::{EncodedGraphName, EncodedQuad, EncodedTerm};
use std::sync::Arc;

/// An owning lexical graph name; literals cannot inhabit this domain.
#[derive(Clone, Debug)]
pub enum LexicalGraphName {
    /// The logical default graph, with no identifier term.
    DefaultGraph,
    /// A named graph IRI.
    NamedNode(TermText),
    /// A blank-node graph identifier.
    BlankNode(TermText),
}

impl LexicalGraphName {
    /// Move a graph identifier into the corresponding optional RDF term.
    pub fn into_term(self) -> Option<LexicalTerm> {
        match self {
            Self::DefaultGraph => None,
            Self::NamedNode(value) => Some(LexicalTerm::NamedNode(value)),
            Self::BlankNode(value) => Some(LexicalTerm::BlankNode(value)),
        }
    }

    fn decode(reader: &StorageReader<'_>, name: EncodedGraphName) -> Result<Self, StorageError> {
        Ok(match name {
            EncodedGraphName::DefaultGraph => Self::DefaultGraph,
            EncodedGraphName::NamedNode { iri_id } => {
                Self::NamedNode(reader.get_term_text(&iri_id)?)
            }
            EncodedGraphName::NumericalBlankNode { id } => Self::BlankNode(TermText::owned(
                BlankNode::new_from_unique_id(u128::from_be_bytes(id)).into_string(),
            )),
            EncodedGraphName::SmallBlankNode(value) => Self::BlankNode(TermText::inline(value)),
            EncodedGraphName::BigBlankNode { id_id } => {
                Self::BlankNode(reader.get_term_text(&id_id)?)
            }
        })
    }
}

/// Full statement terms and graph metadata leasing their immutable lexical words.
#[derive(Debug)]
pub struct LexicalQuad {
    /// Full-Term subject, including literals.
    pub subject: LexicalTerm,
    /// Full-Term predicate, including blanks and literals.
    pub predicate: LexicalTerm,
    /// Full-Term object.
    pub object: LexicalTerm,
    /// Typed default, named or blank graph metadata.
    pub graph_name: LexicalGraphName,
}

impl LexicalQuad {
    pub(super) fn decode(
        reader: &StorageReader<'_>,
        quad: EncodedQuad,
    ) -> Result<Self, StorageError> {
        Ok(Self {
            subject: LexicalTerm::decode(reader, &quad.subject)?,
            predicate: LexicalTerm::decode(reader, &quad.predicate)?,
            object: LexicalTerm::decode(reader, &quad.object)?,
            graph_name: LexicalGraphName::decode(reader, quad.graph_name)?,
        })
    }
}

/// An owning full-quad lexical cursor over the existing generalized relation.
#[must_use]
pub struct GeneralizedLexicalQuadRows<'a> {
    inner: GeneralizedQuadIter<'a>,
}

impl<'a> GeneralizedLexicalQuadRows<'a> {
    pub(super) fn new(inner: GeneralizedQuadIter<'a>) -> Self {
        Self { inner }
    }
}

impl Iterator for GeneralizedLexicalQuadRows<'_> {
    type Item = Result<LexicalQuad, StorageError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next_encoded().map(|quad| {
            let quad = quad?;
            let reader = &self.inner.reader;
            LexicalQuad::decode(reader, quad)
        })
    }
}

/// Owning graph-only lexical projection; lazy patterns open only when consumed.
#[must_use]
pub struct GeneralizedLexicalGraphRows<'a> {
    source: GraphSource<'a>,
}
enum GraphSource<'a> {
    Cursor(GeneralizedQuadIter<'a>),
    Lazy {
        reader: Arc<StorageReader<'a>>,
        pattern: [EncodedTerm; 3],
        iter: Option<DecodingQuadIterator<'a>>,
    },
}
impl<'a> GeneralizedLexicalGraphRows<'a> {
    pub(super) fn new(inner: GeneralizedQuadIter<'a>) -> Self {
        Self {
            source: GraphSource::Cursor(inner),
        }
    }
    fn lazy(reader: Arc<StorageReader<'a>>, pattern: [EncodedTerm; 3]) -> Self {
        Self {
            source: GraphSource::Lazy {
                reader,
                pattern,
                iter: None,
            },
        }
    }
}
impl Iterator for GeneralizedLexicalGraphRows<'_> {
    type Item = Result<LexicalGraphName, StorageError>;
    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.source {
            GraphSource::Cursor(inner) => inner
                .next_encoded()
                .map(|quad| LexicalGraphName::decode(&inner.reader, quad?.graph_name)),
            GraphSource::Lazy {
                reader,
                pattern,
                iter,
            } => {
                let iter = iter.get_or_insert_with(|| {
                    reader.quads_for_pattern(
                        Some(&pattern[0]),
                        Some(&pattern[1]),
                        Some(&pattern[2]),
                        None,
                    )
                });
                iter.next()
                    .map(|quad| LexicalGraphName::decode(reader, quad?.graph_name))
            }
        }
    }
}

/// One union statement and an unconsumed context cursor from its exact reader.
pub struct LexicalUnionRow {
    /// Full statement fields; graph metadata is not restored yet.
    pub terms: [LexicalTerm; 3],
    /// Lazy exact encoded-SPO physical contexts sharing this row's snapshot.
    pub contexts: GeneralizedLexicalGraphRows<'static>,
}

/// Generalized union rows from a committed owning reader.
pub struct GeneralizedLexicalUnionRows {
    inner: GeneralizedQuadIter<'static>,
}
impl GeneralizedLexicalUnionRows {
    pub(super) fn new(inner: GeneralizedQuadIter<'static>) -> Self {
        Self {
            inner: inner.unique_spo(),
        }
    }
}
impl Iterator for GeneralizedLexicalUnionRows {
    type Item = Result<LexicalUnionRow, StorageError>;
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next_encoded().map(|quad| {
            let quad = quad?;
            let reader = &self.inner.reader;
            let terms = [
                LexicalTerm::decode(reader, &quad.subject)?,
                LexicalTerm::decode(reader, &quad.predicate)?,
                LexicalTerm::decode(reader, &quad.object)?,
            ];
            let contexts = GeneralizedLexicalGraphRows::lazy(
                Arc::clone(reader),
                [quad.subject, quad.predicate, quad.object],
            );
            Ok(LexicalUnionRow { terms, contexts })
        })
    }
}
