//! Derived-only private inference over one stable borrowed graph source.
use oxigraph::model::*;
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults};
use pyshacl_turbo_engine::{GraphRead, QueryGraph};

use super::backend::Graph;
use super::{InferenceOutcome, InferenceRequest};
use crate::Result;

type Matches<'a> = Box<dyn Iterator<Item = pyshacl_turbo_engine::Result<Quad>> + 'a>;

pub(crate) struct BorrowedInference<'a, G: GraphRead + ?Sized> {
    source: &'a G,
    additions: Dataset,
    destination: GraphName,
}

impl<'a, G: GraphRead + ?Sized> BorrowedInference<'a, G> {
    pub(crate) fn new(source: &'a G, destination: GraphName) -> Self {
        Self {
            source,
            additions: Dataset::new(),
            destination,
        }
    }

    pub(crate) fn expand(
        &mut self,
        mode: &str,
        request: InferenceRequest,
    ) -> Result<InferenceOutcome> {
        super::expand_configured(self, mode, request).map(|(_, outcome)| outcome)
    }

    pub(crate) fn into_additions(self) -> Dataset {
        self.additions
    }

    fn physical_matches<'b>(
        &'b self,
        subject: Option<NamedOrBlankNodeRef<'_>>,
        predicate: Option<NamedNodeRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> Matches<'b> {
        Box::new(
            self.source
                .matching_quads(subject, predicate, object, graph)
                .chain(
                    self.additions
                        .matching_quads(subject, predicate, object, graph),
                ),
        )
    }

    fn default_matches<'b>(
        &'b self,
        subject: Option<NamedOrBlankNodeRef<'_>>,
        predicate: Option<NamedNodeRef<'_>>,
        object: Option<TermRef<'_>>,
    ) -> Matches<'b> {
        Box::new(
            self.physical_matches(subject, predicate, object, None)
                .filter_map(|quad| {
                    let mut quad = match quad {
                        Ok(quad) => quad,
                        Err(error) => return Some(Err(error)),
                    };
                    // Exact representative probes do not assume Store iteration order.
                    let first = self
                        .physical_matches(
                            Some(quad.subject.as_ref()),
                            Some(quad.predicate.as_ref()),
                            Some(quad.object.as_ref()),
                            None,
                        )
                        .next();
                    match first {
                        Some(Err(error)) => Some(Err(error)),
                        Some(Ok(first)) if first.graph_name == quad.graph_name => {
                            quad.graph_name = GraphName::DefaultGraph;
                            Some(Ok(quad))
                        }
                        Some(Ok(_)) => None,
                        None => Some(Err(pyshacl_turbo_engine::Error::Runtime(
                            "Graph changed during inference union read".into(),
                        ))),
                    }
                }),
        )
    }
}

impl<G: GraphRead + ?Sized> Graph for BorrowedInference<'_, G> {
    fn quads<'a>(
        &'a self,
        subject: Option<NamedOrBlankNodeRef<'a>>,
        predicate: Option<NamedNodeRef<'a>>,
        object: Option<TermRef<'a>>,
    ) -> impl Iterator<Item = Result<Quad>> + 'a {
        self.default_matches(subject, predicate, object)
            .map(|quad| quad.map_err(Into::into))
    }

    fn query(&self, query: PreparedSparqlQuery) -> Result<QueryResults<'_>> {
        // The query adapter owns a borrow of this stable overlay, not a local view.
        Ok(query
            .on_queryable_dataset(QueryGraph::new(self))
            .execute()?)
    }

    fn contains_destination(&self, quad: &Quad) -> Result<bool> {
        self.physical_matches(
            Some(quad.subject.as_ref()),
            Some(quad.predicate.as_ref()),
            Some(quad.object.as_ref()),
            Some(self.destination.as_ref()),
        )
        .next()
        .transpose()
        .map(|quad| quad.is_some())
        .map_err(Into::into)
    }

    fn insert(&mut self, mut quad: Quad) -> Result<()> {
        quad.graph_name = self.destination.clone();
        if self
            .source
            .matching_quads(
                Some(quad.subject.as_ref()),
                Some(quad.predicate.as_ref()),
                Some(quad.object.as_ref()),
                Some(quad.graph_name.as_ref()),
            )
            .next()
            .transpose()?
            .is_none()
        {
            self.additions.insert(&quad);
        }
        Ok(())
    }
}

impl<'source, G: GraphRead + ?Sized> GraphRead for BorrowedInference<'source, G> {
    fn matching_quads<'a>(
        &'a self,
        subject: Option<NamedOrBlankNodeRef<'_>>,
        predicate: Option<NamedNodeRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = pyshacl_turbo_engine::Result<Quad>> + use<'a, 'source, G> {
        let matches: Matches<'a> = match graph {
            Some(GraphNameRef::DefaultGraph) => self.default_matches(subject, predicate, object),
            Some(_) => self.physical_matches(subject, predicate, object, graph),
            None => Box::new(
                self.default_matches(subject, predicate, object).chain(
                    self.physical_matches(subject, predicate, object, None)
                        .filter(|quad| match quad {
                            Ok(quad) => !quad.graph_name.is_default_graph(),
                            Err(_) => true,
                        }),
                ),
            ),
        };
        matches
    }

    fn named_graphs(
        &self,
    ) -> impl Iterator<Item = pyshacl_turbo_engine::Result<NamedOrBlankNode>> + '_ {
        let graph: Option<NamedOrBlankNode> = match &self.destination {
            GraphName::NamedNode(node) => Some(node.clone().into()),
            GraphName::BlankNode(node) => Some(node.clone().into()),
            GraphName::DefaultGraph => None,
        };
        let extra = graph
            .filter(|_| !self.additions.is_empty())
            .and_then(|graph| match self.source.contains_named_graph(&graph) {
                Ok(true) => None,
                Ok(false) => Some(Ok(graph)),
                Err(error) => Some(Err(error)),
            });
        self.source.named_graphs().chain(extra)
    }

    fn contains_named_graph(&self, graph: &NamedOrBlankNode) -> pyshacl_turbo_engine::Result<bool> {
        Ok(
            (self.destination.as_ref() == GraphNameRef::from(graph.as_ref())
                && !self.additions.is_empty())
                || self.source.contains_named_graph(graph)?,
        )
    }
}
