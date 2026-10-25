//! Graph positions retain only their existing default, named and blank domains.
use super::error::CorruptionError;
use super::numeric_encoder::{EncodedTerm, StrHash};
use super::small_string::SmallString;
use crate::model::GraphNameRef;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum EncodedGraphName {
    DefaultGraph,
    NamedNode { iri_id: StrHash },
    NumericalBlankNode { id: [u8; 16] },
    SmallBlankNode(SmallString),
    BigBlankNode { id_id: StrHash },
}

impl EncodedGraphName {
    pub fn is_default_graph(&self) -> bool {
        matches!(self, Self::DefaultGraph)
    }
}

impl TryFrom<EncodedTerm> for EncodedGraphName {
    type Error = CorruptionError;

    fn try_from(term: EncodedTerm) -> Result<Self, Self::Error> {
        Ok(match term {
            EncodedTerm::DefaultGraph => Self::DefaultGraph,
            EncodedTerm::NamedNode { iri_id } => Self::NamedNode { iri_id },
            EncodedTerm::NumericalBlankNode { id } => Self::NumericalBlankNode { id },
            EncodedTerm::SmallBlankNode(value) => Self::SmallBlankNode(value),
            EncodedTerm::BigBlankNode { id_id } => Self::BigBlankNode { id_id },
            _ => return Err(CorruptionError::msg("Invalid encoded graph-name term kind")),
        })
    }
}

impl From<GraphNameRef<'_>> for EncodedGraphName {
    fn from(name: GraphNameRef<'_>) -> Self {
        // Reuse the existing full-width encoding and hash policy. GraphNameRef
        // can represent only the five encoded cases accepted above.
        Self::try_from(EncodedTerm::from(name)).expect("GraphNameRef encodes only graph names")
    }
}

impl From<EncodedGraphName> for EncodedTerm {
    fn from(name: EncodedGraphName) -> Self {
        match name {
            EncodedGraphName::DefaultGraph => Self::DefaultGraph,
            EncodedGraphName::NamedNode { iri_id } => Self::NamedNode { iri_id },
            EncodedGraphName::NumericalBlankNode { id } => Self::NumericalBlankNode { id },
            EncodedGraphName::SmallBlankNode(value) => Self::SmallBlankNode(value),
            EncodedGraphName::BigBlankNode { id_id } => Self::BigBlankNode { id_id },
        }
    }
}

impl PartialEq<EncodedTerm> for EncodedGraphName {
    fn eq(&self, other: &EncodedTerm) -> bool {
        EncodedTerm::from(*self).eq(other)
    }
}

impl PartialEq<EncodedGraphName> for EncodedTerm {
    fn eq(&self, other: &EncodedGraphName) -> bool {
        other.eq(self)
    }
}
