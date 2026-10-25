use super::{predicate, subject, GeneralizedNarrowingError};
use crate::{GraphName, GraphNameRef, Quad, QuadRef, Term, TermRef};

/// A native data quad allowing any RDF term in subject and predicate positions.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct GeneralizedQuad {
    /// The subject term.
    pub subject: Term,
    /// The predicate term.
    pub predicate: Term,
    /// The object term.
    pub object: Term,
    /// The graph name term.
    pub graph_name: GraphName,
}

impl GeneralizedQuad {
    /// Builds a generalized data quad without narrowing its terms.
    pub fn new(
        subject: impl Into<Term>,
        predicate: impl Into<Term>,
        object: impl Into<Term>,
        graph_name: impl Into<GraphName>,
    ) -> Self {
        Self {
            subject: subject.into(),
            predicate: predicate.into(),
            object: object.into(),
            graph_name: graph_name.into(),
        }
    }

    /// Borrows the data terms without copying their payloads.
    pub fn as_ref(&self) -> GeneralizedQuadRef<'_> {
        GeneralizedQuadRef {
            subject: self.subject.as_ref(),
            predicate: self.predicate.as_ref(),
            object: self.object.as_ref(),
            graph_name: self.graph_name.as_ref(),
        }
    }
}

/// Borrowed generalized data terms, independent of strict RDF syntax positions.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct GeneralizedQuadRef<'a> {
    /// The borrowed subject term.
    pub subject: TermRef<'a>,
    /// The borrowed predicate term.
    pub predicate: TermRef<'a>,
    /// The borrowed object term.
    pub object: TermRef<'a>,
    /// The borrowed graph name term.
    pub graph_name: GraphNameRef<'a>,
}

impl GeneralizedQuadRef<'_> {
    /// Copies the borrowed terms into an owned value.
    pub fn into_owned(self) -> GeneralizedQuad {
        GeneralizedQuad {
            subject: self.subject.into_owned(),
            predicate: self.predicate.into_owned(),
            object: self.object.into_owned(),
            graph_name: self.graph_name.into_owned(),
        }
    }
}

impl From<Quad> for GeneralizedQuad {
    fn from(value: Quad) -> Self {
        Self {
            subject: value.subject.into(),
            predicate: value.predicate.into(),
            object: value.object,
            graph_name: value.graph_name,
        }
    }
}

impl<'a> From<QuadRef<'a>> for GeneralizedQuadRef<'a> {
    fn from(value: QuadRef<'a>) -> Self {
        Self {
            subject: value.subject.into(),
            predicate: value.predicate.into(),
            object: value.object,
            graph_name: value.graph_name,
        }
    }
}

impl<'a> From<&'a GeneralizedQuad> for GeneralizedQuadRef<'a> {
    fn from(value: &'a GeneralizedQuad) -> Self {
        value.as_ref()
    }
}

impl<'a> From<&'a Quad> for GeneralizedQuadRef<'a> {
    fn from(value: &'a Quad) -> Self {
        value.as_ref().into()
    }
}

impl<'a> TryFrom<GeneralizedQuadRef<'a>> for QuadRef<'a> {
    type Error = GeneralizedNarrowingError;

    fn try_from(value: GeneralizedQuadRef<'a>) -> Result<Self, Self::Error> {
        Ok(Self {
            subject: subject(value.subject)?,
            predicate: predicate(value.predicate)?,
            object: value.object,
            graph_name: value.graph_name,
        })
    }
}

impl TryFrom<GeneralizedQuad> for Quad {
    type Error = GeneralizedNarrowingError;

    fn try_from(value: GeneralizedQuad) -> Result<Self, Self::Error> {
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
            graph_name: value.graph_name,
        })
    }
}
