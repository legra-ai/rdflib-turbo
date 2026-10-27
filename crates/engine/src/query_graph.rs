//! SPARQL adapter over a borrowed graph without copying the logical dataset.
use std::collections::HashMap;

use oxigraph::model::*;
use spareval::{InternalQuad, QueryableDataset, ScopedCustomFunction, TermCow};

use crate::{Error, GraphRead, Result};

/// Evaluate SPARQL over the exact graph contexts exposed by a borrowed source.
/// Compose union/overlay views explicitly before constructing this adapter.
/// The adapter adds no default union, snapshots, or graph-name collections.
/// See the [crate architecture and runnable example](crate#borrowed-sparql-graph-architecture).
pub struct QueryGraph<'a, G: GraphRead + ?Sized> {
    source: &'a G,
    // bounded: one callback per function declared in the compiled shapes graph.
    functions: HashMap<NamedNode, ScopedCustomFunction<'a>>,
}

impl<'a, G: GraphRead + ?Sized> QueryGraph<'a, G> {
    fn subject(term: TermRef<'_>) -> Option<NamedOrBlankNodeRef<'_>> {
        match term {
            TermRef::NamedNode(node) => Some(node.into()),
            TermRef::BlankNode(node) => Some(node.into()),
            _ => None,
        }
    }

    /// Borrow a consistent graph source for the lifetime of query evaluation.
    pub fn new(source: &'a G) -> Self {
        Self {
            source,
            functions: HashMap::new(),
        }
    }
    pub(crate) fn with_functions(
        source: &'a G,
        functions: HashMap<NamedNode, ScopedCustomFunction<'a>>,
    ) -> Self {
        Self { source, functions }
    }
}

impl<'a, G: GraphRead + ?Sized + 'a> QueryableDataset<'a> for QueryGraph<'a, G> {
    type InternalTerm = TermCow<'a>;
    type Error = Error;

    fn scoped_custom_function(&self, name: &NamedNode) -> Option<ScopedCustomFunction<'a>> {
        self.functions.get(name).cloned()
    }

    fn internal_quads_for_pattern(
        &self,
        subject: Option<&TermCow<'a>>,
        predicate: Option<&TermCow<'a>>,
        object: Option<&TermCow<'a>>,
        graph_name: Option<Option<&TermCow<'a>>>,
    ) -> impl Iterator<Item = Result<InternalQuad<TermCow<'a>>>> + use<'a, G> {
        let subject = subject.map(TermRef::from);
        let predicate = predicate.map(TermRef::from);
        let graph = match graph_name {
            Some(Some(term)) => match Self::subject(TermRef::from(term)) {
                Some(value) => Some(GraphNameRef::from(value)),
                None => return Box::new(std::iter::empty()) as Box<dyn Iterator<Item = _>>,
            },
            Some(None) => Some(GraphNameRef::DefaultGraph),
            None => None,
        };
        // SPARQL's wildcard graph means named graphs only; GraphRead includes
        // the default graph when its graph filter is absent.
        let named_only = graph_name.is_none();
        Box::new(
            self.source
                .matching_query_quads(subject, predicate, object.map(TermRef::from), graph)
                .filter(move |quad| {
                    !named_only
                        || match quad {
                            Ok(quad) => quad.graph_name.is_some(),
                            Err(_) => true,
                        }
                }),
        )
    }

    fn internal_named_graphs(&self) -> impl Iterator<Item = Result<TermCow<'a>>> + use<'a, G> {
        self.source
            .named_graphs()
            .map(|graph| graph.map(|graph| Term::from(graph).into()))
    }

    fn contains_internal_graph_name(&self, graph: &TermCow<'a>) -> Result<bool> {
        match Self::subject(TermRef::from(graph)) {
            Some(graph) => self.source.contains_named_graph(&graph.into_owned()),
            None => Ok(false),
        }
    }

    fn internalize_term(&self, term: Term) -> Result<TermCow<'a>> {
        Ok(term.into())
    }

    fn externalize_term(&self, term: TermCow<'a>) -> Result<Term> {
        Ok(term.into())
    }
}

#[cfg(test)]
#[path = "query_borrowed_tests.rs"]
mod borrowed_tests;

pub(crate) fn owned_quad(quad: GeneralizedQuad) -> InternalQuad<TermCow<'static>> {
    InternalQuad {
        subject: quad.subject.into(),
        predicate: quad.predicate.into(),
        object: quad.object.into(),
        graph_name: match quad.graph_name {
            GraphName::NamedNode(node) => Some(Term::from(node).into()),
            GraphName::BlankNode(node) => Some(Term::from(node).into()),
            GraphName::DefaultGraph => None,
        },
    }
}

pub(crate) fn borrowed_quad(quad: GeneralizedQuadRef<'_>) -> InternalQuad<TermCow<'_>> {
    InternalQuad {
        subject: quad.subject.into(),
        predicate: quad.predicate.into(),
        object: quad.object.into(),
        graph_name: match quad.graph_name {
            GraphNameRef::NamedNode(node) => Some(TermRef::from(node).into()),
            GraphNameRef::BlankNode(node) => Some(TermRef::from(node).into()),
            GraphNameRef::DefaultGraph => None,
        },
    }
}
