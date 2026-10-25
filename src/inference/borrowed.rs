//! Derived-only private inference over one stable borrowed graph source.
use oxigraph::model::*;
use oxigraph::sparql::PreparedSparqlQuery;
#[cfg(test)]
use oxigraph::sparql::QueryResults;
use oxrdf::GeneralizedDataset;
use pyshacl_turbo_engine::{GraphRead, QueryGraph};

use super::backend::Graph;
use super::{InferenceDelta, InferenceOutcome, InferenceRequest};
use crate::Result;

type Matches<'a> = Box<dyn Iterator<Item = pyshacl_turbo_engine::Result<GeneralizedQuad>> + 'a>;

pub(crate) struct BorrowedInference<'a, G: GraphRead + ?Sized> {
    source: &'a G,
    delta: InferenceDelta,
    destination: GraphName,
}

impl<'a, G: GraphRead + ?Sized> BorrowedInference<'a, G> {
    pub(crate) fn new(source: &'a G, destination: GraphName) -> Self {
        Self {
            source,
            delta: InferenceDelta::default(),
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

    pub(crate) fn into_delta(self) -> InferenceDelta {
        self.delta
    }

    fn physical_matches<'b>(
        &'b self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> Matches<'b> {
        Box::new(
            self.source
                .matching_quads(subject, predicate, object, graph)
                .filter(|quad| match quad {
                    Ok(quad) => !self.delta.is_removed(quad.as_ref()),
                    Err(_) => true,
                })
                .chain(
                    self.delta
                        .additions
                        .matching_quads(subject, predicate, object, graph),
                ),
        )
    }

    fn default_matches<'b>(
        &'b self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
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

impl<'source, G: GraphRead + ?Sized> Graph for BorrowedInference<'source, G> {
    fn quads<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
    ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a, 'source, G> {
        self.default_matches(subject, predicate, object)
            .map(|quad| Ok(quad?))
    }

    #[cfg(test)]
    fn query(&self, query: PreparedSparqlQuery) -> Result<QueryResults<'_>> {
        // The query adapter owns a borrow of this stable overlay, not a local view.
        Ok(query
            .on_queryable_dataset(QueryGraph::new(self))
            .execute()?)
    }

    fn inference_heads(
        &self,
        query: PreparedSparqlQuery,
    ) -> Result<oxigraph::sparql::QueryTripleIter<'_, GeneralizedTriple>> {
        Ok(query
            .on_queryable_dataset(QueryGraph::new(self))
            .execute_inference_construct()?)
    }

    fn cleanup_blank_predicates(&mut self) -> Result<()> {
        // Source rows remain immutable. Retain only actual full-Term removals;
        // errors abort staging rather than disappearing through the filter.
        let mut removals: Option<Box<GeneralizedDataset>> = None;
        for quad in self.source.matching_quads(None, None, None, None) {
            let quad = quad?;
            if quad.predicate.is_blank_node() {
                removals.get_or_insert_with(Default::default).insert(&quad);
            }
        }
        self.delta.retain_removals(removals);
        let mut transaction = self.delta.additions.transaction();
        transaction.remove_where(|quad| quad.predicate.is_blank_node());
        transaction.commit();
        Ok(())
    }

    fn contains(&self, quad: GeneralizedQuadRef<'_>) -> Result<bool> {
        // Probe the source first so an overlay hit cannot hide its read error.
        if self
            .source
            .matching_quads(
                Some(quad.subject),
                Some(quad.predicate),
                Some(quad.object),
                None,
            )
            .find(|quad| match quad {
                Ok(quad) => !self.delta.is_removed(quad.as_ref()),
                Err(_) => true,
            })
            .transpose()?
            .is_some()
        {
            return Ok(true);
        }
        // Every overlay insertion uses exactly self.destination. A boolean
        // needs no owned row or representative from the logical union.
        Ok(self.delta.additions.contains(GeneralizedQuadRef {
            graph_name: self.destination.as_ref(),
            ..quad
        }))
    }

    fn contains_destination(&self, quad: GeneralizedQuadRef<'_>) -> Result<bool> {
        if self
            .source
            .matching_quads(
                Some(quad.subject),
                Some(quad.predicate),
                Some(quad.object),
                Some(self.destination.as_ref()),
            )
            .find(|quad| match quad {
                Ok(quad) => !self.delta.is_removed(quad.as_ref()),
                Err(_) => true,
            })
            .transpose()?
            .is_some()
        {
            return Ok(true);
        }
        Ok(self.delta.additions.contains(GeneralizedQuadRef {
            graph_name: self.destination.as_ref(),
            ..quad
        }))
    }

    fn insert(&mut self, mut quad: GeneralizedQuad) -> Result<()> {
        quad.graph_name = self.destination.clone();
        if self
            .source
            .matching_quads(
                Some(quad.subject.as_ref()),
                Some(quad.predicate.as_ref()),
                Some(quad.object.as_ref()),
                Some(quad.graph_name.as_ref()),
            )
            .find(|quad| match quad {
                Ok(quad) => !self.delta.is_removed(quad.as_ref()),
                Err(_) => true,
            })
            .transpose()?
            .is_none()
        {
            self.delta.additions.insert(&quad);
        }
        Ok(())
    }
}

impl<'source, G: GraphRead + ?Sized> GraphRead for BorrowedInference<'source, G> {
    fn matching_quads<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = pyshacl_turbo_engine::Result<GeneralizedQuad>> + use<'a, 'source, G>
    {
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
            .filter(|_| !self.delta.additions.is_empty())
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
                && !self.delta.additions.is_empty())
                || self.source.contains_named_graph(graph)?,
        )
    }
}

#[cfg(test)]
#[path = "borrowed_boolean_tests.rs"]
mod boolean_tests;
