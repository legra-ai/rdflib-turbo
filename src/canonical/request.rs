//! Typed canonicalization requests parsed once at the Python boundary.
use std::num::NonZeroU64;

use oxigraph::model::dataset::{CanonicalizationAlgorithm, CanonicalizationHashAlgorithm};
use oxigraph::model::{CanonicalizationBudget, GraphName};

use crate::{Error, Result};

/// Which quads one canonicalization reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CanonicalScope {
    /// Every quad of the dataset, keeping graph names.
    Dataset,
    /// One graph's triples, canonicalized as a default-graph dataset.
    Graph(GraphName),
}

impl CanonicalScope {
    /// The graph pattern selecting this scope's quads.
    pub(crate) fn pattern(&self) -> Option<oxigraph::model::GraphNameRef<'_>> {
        match self {
            Self::Dataset => None,
            Self::Graph(graph) => Some(graph.as_ref()),
        }
    }

    /// Whether output quads keep their graph names.
    pub(crate) fn keeps_graph_names(&self) -> bool {
        matches!(self, Self::Dataset)
    }
}

/// How literal identity enters the canonical form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TermIdentity {
    /// RDF 1.1 term identity, as RDFC-1.0 requires: an explicit `xsd:string`
    /// literal is the same term as the simple literal with its lexical form.
    Rdf11,
    /// RDFLib term identity: explicit `xsd:string` stays distinct.
    Exact,
}

impl TermIdentity {
    fn parse(source: &str) -> Result<Self> {
        match source {
            "rdf11" => Ok(Self::Rdf11),
            "exact" => Ok(Self::Exact),
            other => Err(Error::Invalid(format!("Unknown term identity: {other:?}"))),
        }
    }
}

/// The RDFC-1.0 hash function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CanonicalHash {
    Sha256,
    Sha384,
}

impl CanonicalHash {
    fn parse(source: &str) -> Result<Self> {
        match source {
            "sha256" => Ok(Self::Sha256),
            "sha384" => Ok(Self::Sha384),
            other => Err(Error::Invalid(format!(
                "Unsupported RDFC-1.0 hash algorithm: {other:?}; expected 'sha256' or 'sha384'"
            ))),
        }
    }

    fn algorithm(self) -> CanonicalizationAlgorithm {
        CanonicalizationAlgorithm::Rdfc10 {
            hash_algorithm: match self {
                Self::Sha256 => CanonicalizationHashAlgorithm::Sha256,
                Self::Sha384 => CanonicalizationHashAlgorithm::Sha384,
            },
        }
    }
}

/// One validated canonicalization request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CanonicalRequest {
    hash: CanonicalHash,
    budget: NonZeroU64,
    identity: TermIdentity,
}

impl CanonicalRequest {
    /// Parses the Python-facing hash name, positive work budget and identity mode.
    pub(crate) fn try_new(hash: &str, budget: u64, identity: &str) -> Result<Self> {
        Ok(Self {
            hash: CanonicalHash::parse(hash)?,
            budget: NonZeroU64::new(budget).ok_or_else(|| {
                Error::Invalid("The RDFC-1.0 work budget must be positive".into())
            })?,
            identity: TermIdentity::parse(identity)?,
        })
    }

    pub(crate) fn algorithm(self) -> CanonicalizationAlgorithm {
        self.hash.algorithm()
    }

    pub(crate) fn budget(self) -> CanonicalizationBudget {
        CanonicalizationBudget::limited(self.budget)
    }

    pub(crate) fn identity(self) -> TermIdentity {
        self.identity
    }
}
