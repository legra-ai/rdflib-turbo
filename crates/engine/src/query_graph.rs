//! SPARQL adapter over a borrowed graph without copying the logical dataset.
use std::collections::HashMap;

use oxigraph::model::*;
use spareval::{InternalQuad, QueryableDataset, ScopedCustomFunction};

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
    fn subject(term: &Term) -> Option<NamedOrBlankNodeRef<'_>> {
        match term {
            Term::NamedNode(node) => Some(node.as_ref().into()),
            Term::BlankNode(node) => Some(node.as_ref().into()),
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
    type InternalTerm = Term;
    type Error = Error;

    fn scoped_custom_function(&self, name: &NamedNode) -> Option<ScopedCustomFunction<'a>> {
        self.functions.get(name).cloned()
    }

    fn internal_quads_for_pattern(
        &self,
        subject: Option<&Term>,
        predicate: Option<&Term>,
        object: Option<&Term>,
        graph_name: Option<Option<&Term>>,
    ) -> impl Iterator<Item = Result<InternalQuad<Term>>> + use<'a, G> {
        let subject = subject.map(Term::as_ref);
        let predicate = predicate.map(Term::as_ref);
        let graph = match graph_name {
            Some(Some(term)) => match Self::subject(term) {
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
                .matching_quads(subject, predicate, object.map(Term::as_ref), graph)
                .filter(move |quad| {
                    !named_only
                        || match quad {
                            Ok(q) => !q.graph_name.is_default_graph(),
                            Err(_) => true,
                        }
                })
                .map(|quad| {
                    quad.map(|quad| InternalQuad {
                        subject: quad.subject,
                        predicate: quad.predicate,
                        object: quad.object,
                        graph_name: match quad.graph_name {
                            GraphName::NamedNode(node) => Some(node.into()),
                            GraphName::BlankNode(node) => Some(node.into()),
                            GraphName::DefaultGraph => None,
                        },
                    })
                }),
        )
    }

    fn internal_named_graphs(&self) -> impl Iterator<Item = Result<Term>> + use<'a, G> {
        self.source
            .named_graphs()
            .map(|graph| graph.map(Into::into))
    }

    fn contains_internal_graph_name(&self, graph: &Term) -> Result<bool> {
        match Self::subject(graph) {
            Some(graph) => self.source.contains_named_graph(&graph.into_owned()),
            None => Ok(false),
        }
    }

    fn internalize_term(&self, term: Term) -> Result<Term> {
        Ok(term)
    }

    fn externalize_term(&self, term: Term) -> Result<Term> {
        Ok(term)
    }
}
