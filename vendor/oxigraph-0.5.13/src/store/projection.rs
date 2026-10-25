//! Decode only requested statement terms from an existing storage cursor.
use super::{QuadIter, StorageError};
use crate::model::Term;
use crate::storage::numeric_encoder::Decoder;

/// An existing quad scan that decodes only selected subject, predicate and object terms.
///
/// This iterator retains its source cursor's snapshot and borrowing lifetime. It does
/// not decode graph names, create another snapshot or materialize scan results.
#[must_use]
pub struct ProjectedQuadIter<'a> {
    inner: QuadIter<'a>,
    selected: [bool; 3],
}

impl<'a> QuadIter<'a> {
    /// Consumes this cursor and decodes only the selected statement positions.
    ///
    /// Results contain subject, predicate and object slots in that order. An
    /// unselected slot is `None`; selecting no slots still reports one result per
    /// matching quad. Graph names are never decoded. Existing storage errors and
    /// errors decoding selected terms are preserved.
    pub fn project(self, subject: bool, predicate: bool, object: bool) -> ProjectedQuadIter<'a> {
        ProjectedQuadIter {
            inner: self,
            selected: [subject, predicate, object],
        }
    }
}

impl Iterator for ProjectedQuadIter<'_> {
    type Item = Result<[Option<Term>; 3], StorageError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.iter.next().map(|quad| {
            let quad = quad?;
            let reader = &self.inner.reader;
            Ok([
                self.selected[0]
                    .then(|| {
                        reader
                            .decode_named_or_blank_node(&quad.subject)
                            .map(Term::from)
                    })
                    .transpose()?,
                self.selected[1]
                    .then(|| reader.decode_named_node(&quad.predicate).map(Term::from))
                    .transpose()?,
                self.selected[2]
                    .then(|| reader.decode_term(&quad.object))
                    .transpose()?,
            ])
        })
    }
}
