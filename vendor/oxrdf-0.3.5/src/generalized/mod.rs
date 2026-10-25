//! Data terms with explicit checked boundaries to strict RDF syntax.

mod quad;
mod triple;

use crate::{NamedNodeRef, NamedOrBlankNodeRef, TermRef};
pub use quad::{GeneralizedQuad, GeneralizedQuadRef};
use std::{error::Error, fmt};
pub use triple::{GeneralizedTriple, GeneralizedTripleRef};

/// The first generalized position that cannot be represented in strict RDF.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeneralizedTermPosition {
    /// A subject that is neither an IRI nor a blank node.
    Subject,
    /// A predicate that is not an IRI.
    Predicate,
}

/// A checked narrowing failure, retaining no potentially large term payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeneralizedNarrowingError(GeneralizedTermPosition);

impl GeneralizedNarrowingError {
    /// Identifies a position rejected before decoding its term payload.
    pub const fn new(position: GeneralizedTermPosition) -> Self {
        Self(position)
    }

    /// Returns the first invalid position, checking subject before predicate.
    pub fn position(self) -> GeneralizedTermPosition {
        self.0
    }
}

impl fmt::Display for GeneralizedNarrowingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self.0 {
            GeneralizedTermPosition::Subject => "RDF subject must be an IRI or blank node",
            GeneralizedTermPosition::Predicate => "RDF predicate must be an IRI",
        })
    }
}

impl Error for GeneralizedNarrowingError {}

fn subject(term: TermRef<'_>) -> Result<NamedOrBlankNodeRef<'_>, GeneralizedNarrowingError> {
    match term {
        TermRef::NamedNode(node) => Ok(node.into()),
        TermRef::BlankNode(node) => Ok(node.into()),
        _ => Err(GeneralizedNarrowingError(GeneralizedTermPosition::Subject)),
    }
}

fn predicate(term: TermRef<'_>) -> Result<NamedNodeRef<'_>, GeneralizedNarrowingError> {
    match term {
        TermRef::NamedNode(node) => Ok(node),
        _ => Err(GeneralizedNarrowingError(
            GeneralizedTermPosition::Predicate,
        )),
    }
}
