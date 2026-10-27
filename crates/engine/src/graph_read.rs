//! Borrowed indexed reads shared by dataset and transaction-backed traversal.
use oxigraph::model::*;
use oxigraph::store::{ReadSnapshot, Transaction};
use spareval::{InternalQuad, TermCow};

use crate::{Error, Result};

/// A fallible stream of stored quads, with explicit graph matching.
///
/// `None` matches every graph; `Some(DefaultGraph)` matches only the default
/// graph. Implementations preserve stored RDF terms and graph contexts. This
/// interface neither constructs a graph union nor copies the base dataset.
/// Storage owners remain responsible for exact-term encoding: a reader cannot
/// recover lexical forms already normalized by its storage backend.
/// During a consistent read, identical patterns must yield the same first quad;
/// no lexical sorting is required. Borrowed views use this stable representative
/// to deduplicate graph contexts without retaining all previously visited triples.
pub trait GraphRead {
    /// Stream matching quads. Read failures must remain errors, never omissions.
    /// Retain the source borrow, not the caller's temporary pattern references.
    fn matching_quads<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a, Self>;

    /// Stream query rows, borrowing dictionary words where the source permits it.
    /// The physical pattern, stable-first and error contracts equal `matching_quads`.
    /// Rows borrow the source, never the caller's temporary pattern references.
    fn matching_query_quads<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = Result<InternalQuad<TermCow<'a>>>> + use<'a, Self> {
        self.matching_quads(subject, predicate, object, graph)
            .map(|quad| quad.map(crate::query_graph::owned_quad))
    }

    /// Stream each named graph exactly once, including empty backend graphs.
    /// Iteration must not materialize the set of graph names.
    fn named_graphs(&self) -> impl Iterator<Item = Result<NamedOrBlankNode>> + '_;

    /// Test graph existence, including empty graphs in capable storage backends.
    fn contains_named_graph(&self, graph: &NamedOrBlankNode) -> Result<bool>;
}

impl GraphRead for Dataset {
    fn matching_quads<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a> {
        // This legacy syntax dataset cannot store generalized positions. An
        // unrepresentable bound term therefore has no matching row here;
        // native snapshots/transactions below keep the full term domain.
        let subject: Option<NamedOrBlankNodeRef<'_>> = match subject {
            Some(TermRef::NamedNode(node)) => Some(node.into()),
            Some(TermRef::BlankNode(node)) => Some(node.into()),
            Some(_) => return Box::new(std::iter::empty()) as Box<dyn Iterator<Item = _>>,
            None => None,
        };
        let predicate = match predicate {
            Some(TermRef::NamedNode(node)) => Some(node),
            Some(_) => return Box::new(std::iter::empty()),
            None => None,
        };
        Box::new(
            self.quads_for_pattern(subject, predicate, object, graph)
                .map(|quad| Ok(GeneralizedQuad::from(quad.into_owned()))),
        )
    }

    fn matching_query_quads<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = Result<InternalQuad<TermCow<'a>>>> + use<'a> {
        let subject: Option<NamedOrBlankNodeRef<'_>> = match subject {
            Some(TermRef::NamedNode(node)) => Some(node.into()),
            Some(TermRef::BlankNode(node)) => Some(node.into()),
            Some(_) => return Box::new(std::iter::empty()) as Box<dyn Iterator<Item = _>>,
            None => None,
        };
        let predicate = match predicate {
            Some(TermRef::NamedNode(node)) => Some(node),
            Some(_) => return Box::new(std::iter::empty()),
            None => None,
        };
        Box::new(
            self.quads_for_pattern(subject, predicate, object, graph)
                .map(|quad| Ok(crate::query_graph::borrowed_quad(quad.into()))),
        )
    }

    fn named_graphs(&self) -> impl Iterator<Item = Result<NamedOrBlankNode>> + '_ {
        // Select one indexed representative per graph; retain no graph-name set.
        self.iter().filter_map(|quad| {
            let name = match quad.graph_name {
                GraphNameRef::NamedNode(node) => NamedOrBlankNode::from(node.into_owned()),
                GraphNameRef::BlankNode(node) => NamedOrBlankNode::from(node.into_owned()),
                GraphNameRef::DefaultGraph => return None,
            };
            (self.quads_for_graph_name(quad.graph_name).next() == Some(quad)).then_some(Ok(name))
        })
    }
    fn contains_named_graph(&self, graph: &NamedOrBlankNode) -> Result<bool> {
        Ok(self.quads_for_graph_name(graph.as_ref()).next().is_some())
    }
}

impl<'tx> GraphRead for Transaction<'tx> {
    fn matching_quads<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a, 'tx> {
        self.generalized_quads_for_pattern(subject, predicate, object, graph)
            .map(|quad| {
                quad.map_err(|error| Error::Runtime(format!("Storage read failed: {error}")))
            })
    }

    fn named_graphs(&self) -> impl Iterator<Item = Result<NamedOrBlankNode>> + '_ {
        Transaction::named_graphs(self).map(|graph| {
            graph.map_err(|error| Error::Runtime(format!("Storage read failed: {error}")))
        })
    }

    fn contains_named_graph(&self, graph: &NamedOrBlankNode) -> Result<bool> {
        Transaction::contains_named_graph(self, graph.as_ref())
            .map_err(|error| Error::Runtime(format!("Storage read failed: {error}")))
    }
}

impl GraphRead for ReadSnapshot {
    fn matching_quads<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a> {
        self.generalized_quads_for_pattern(subject, predicate, object, graph)
            .map(|quad| {
                quad.map_err(|error| Error::Runtime(format!("Storage read failed: {error}")))
            })
    }

    fn named_graphs(&self) -> impl Iterator<Item = Result<NamedOrBlankNode>> + '_ {
        ReadSnapshot::named_graphs(self).map(|graph| {
            graph.map_err(|error| Error::Runtime(format!("Storage read failed: {error}")))
        })
    }

    fn contains_named_graph(&self, graph: &NamedOrBlankNode) -> Result<bool> {
        ReadSnapshot::contains_named_graph(self, graph.as_ref())
            .map_err(|error| Error::Runtime(format!("Storage read failed: {error}")))
    }
}

#[cfg(test)]
#[path = "graph_read_generalized_tests.rs"]
mod generalized_tests;

#[path = "graph_read_generalized.rs"]
mod generalized;
