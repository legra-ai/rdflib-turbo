use super::{predicate, subject, GeneralizedNarrowingError};
use crate::{Term, TermRef, Triple, TripleRef};

/// A native data triple allowing any RDF term in subject and predicate positions.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct GeneralizedTriple {
    /// The subject term.
    pub subject: Term,
    /// The predicate term.
    pub predicate: Term,
    /// The object term.
    pub object: Term,
}

impl GeneralizedTriple {
    /// Builds a generalized data triple without narrowing its terms.
    pub fn new(
        subject: impl Into<Term>,
        predicate: impl Into<Term>,
        object: impl Into<Term>,
    ) -> Self {
        Self {
            subject: subject.into(),
            predicate: predicate.into(),
            object: object.into(),
        }
    }

    /// Borrows the data terms without copying their payloads.
    pub fn as_ref(&self) -> GeneralizedTripleRef<'_> {
        GeneralizedTripleRef {
            subject: self.subject.as_ref(),
            predicate: self.predicate.as_ref(),
            object: self.object.as_ref(),
        }
    }
}

/// Borrowed generalized data terms, independent of strict RDF syntax positions.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct GeneralizedTripleRef<'a> {
    /// The borrowed subject term.
    pub subject: TermRef<'a>,
    /// The borrowed predicate term.
    pub predicate: TermRef<'a>,
    /// The borrowed object term.
    pub object: TermRef<'a>,
}

impl GeneralizedTripleRef<'_> {
    /// Copies the borrowed terms into an owned value.
    pub fn into_owned(self) -> GeneralizedTriple {
        GeneralizedTriple {
            subject: self.subject.into_owned(),
            predicate: self.predicate.into_owned(),
            object: self.object.into_owned(),
        }
    }
}

impl From<Triple> for GeneralizedTriple {
    fn from(value: Triple) -> Self {
        Self {
            subject: value.subject.into(),
            predicate: value.predicate.into(),
            object: value.object,
        }
    }
}

impl<'a> From<TripleRef<'a>> for GeneralizedTripleRef<'a> {
    fn from(value: TripleRef<'a>) -> Self {
        Self {
            subject: value.subject.into(),
            predicate: value.predicate.into(),
            object: value.object,
        }
    }
}

impl<'a> From<&'a GeneralizedTriple> for GeneralizedTripleRef<'a> {
    fn from(value: &'a GeneralizedTriple) -> Self {
        value.as_ref()
    }
}

impl<'a> From<&'a Triple> for GeneralizedTripleRef<'a> {
    fn from(value: &'a Triple) -> Self {
        value.as_ref().into()
    }
}

impl<'a> TryFrom<GeneralizedTripleRef<'a>> for TripleRef<'a> {
    type Error = GeneralizedNarrowingError;

    fn try_from(value: GeneralizedTripleRef<'a>) -> Result<Self, Self::Error> {
        Ok(Self {
            subject: subject(value.subject)?,
            predicate: predicate(value.predicate)?,
            object: value.object,
        })
    }
}

impl TryFrom<GeneralizedTriple> for Triple {
    type Error = GeneralizedNarrowingError;

    fn try_from(value: GeneralizedTriple) -> Result<Self, Self::Error> {
        // Field evaluation checks subject before predicate and moves payloads.
        Ok(Self {
            subject: value
                .subject
                .try_into()
                .map_err(|_| GeneralizedNarrowingError(super::GeneralizedTermPosition::Subject))?,
            predicate: value.predicate.try_into().map_err(|_| {
                GeneralizedNarrowingError(super::GeneralizedTermPosition::Predicate)
            })?,
            object: value.object,
        })
    }
}
