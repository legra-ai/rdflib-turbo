//! Full-Term lexical projection over the existing owning generalized cursor.
use super::super::{LexicalTerm, StorageError};
use super::GeneralizedQuadIter;

/// Selected full-Term fields retaining their original immutable lexical owners.
///
/// The existing snapshot and cursor are moved into this projection. No dictionary
/// or statement relation is copied, and unselected fields are not decoded.
#[must_use]
pub struct GeneralizedLexicalQuadIter<'a> {
    inner: GeneralizedQuadIter<'a>,
    selected: [bool; 3],
}

impl<'a> GeneralizedLexicalQuadIter<'a> {
    pub(super) fn new(inner: GeneralizedQuadIter<'a>, selected: [bool; 3]) -> Self {
        Self { inner, selected }
    }
}

impl Iterator for GeneralizedLexicalQuadIter<'_> {
    type Item = Result<[Option<LexicalTerm>; 3], StorageError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next_encoded().map(|quad| {
            super::super::lexical::project::<true>(&self.inner.reader, &quad?, self.selected)
        })
    }
}
