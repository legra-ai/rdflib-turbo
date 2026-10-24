//! Infer through the existing insertion journal and a borrowed logical union.

use oxigraph::model::*;
use oxigraph::sparql::PreparedSparqlQuery;
#[cfg(test)]
use oxigraph::sparql::QueryResults;
use oxrdf::{GeneralizedDataset, GeneralizedTransaction};

use super::backend::Graph;
use super::{InferenceOutcome, InferenceRequest, expand_configured};
use crate::Result;

#[path = "staged_query.rs"]
mod query;

#[cfg(all(test, feature = "allocation-profiling"))]
#[path = "staged_allocation_tests.rs"]
mod allocation_tests;

/// Read-only virtual default graph over the transaction's existing dictionary.
#[derive(Clone, Copy)]
struct StagedDataset<'a> {
    source: &'a GeneralizedDataset,
}

impl<'a> StagedDataset<'a> {
    fn default_refs(
        &self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
    ) -> impl Iterator<Item = GeneralizedQuadRef<'a>> + use<'a> {
        // bounded: one borrowed previous SPO; native unscoped indexes order graph last.
        let mut previous = None;
        self.source
            .quads_for_pattern(subject, predicate, object, None)
            .filter(move |quad| {
                let triple = GeneralizedTripleRef {
                    subject: quad.subject,
                    predicate: quad.predicate,
                    object: quad.object,
                };
                previous.replace(triple) != Some(triple)
            })
            .map(|quad| GeneralizedQuadRef {
                graph_name: GraphNameRef::DefaultGraph,
                ..quad
            })
    }

    fn matching_refs(
        &self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> Box<dyn Iterator<Item = GeneralizedQuadRef<'a>> + 'a> {
        match graph {
            Some(GraphNameRef::DefaultGraph) => {
                Box::new(self.default_refs(subject, predicate, object))
            }
            Some(graph) => {
                Box::new(
                    self.source
                        .quads_for_pattern(subject, predicate, object, Some(graph)),
                )
            }
            None => Box::new(
                self.default_refs(subject, predicate, object).chain(
                    self.source
                        .quads_for_pattern(subject, predicate, object, None)
                        .filter(|quad| !quad.graph_name.is_default_graph()),
                ),
            ),
        }
    }
}

pub(super) struct JournalGraph<'a> {
    transaction: GeneralizedTransaction<'a>,
    destination: GraphName,
}

impl<'a> JournalGraph<'a> {
    pub(super) fn new(source: &'a mut GeneralizedDataset, destination: GraphName) -> Self {
        Self {
            transaction: source.transaction(),
            destination,
        }
    }

    pub(super) fn inserted(&self) -> impl Iterator<Item = GeneralizedQuadRef<'_>> {
        self.transaction.inserted()
    }

    pub(super) fn commit(self) {
        self.transaction.commit();
    }
}

impl<'data> Graph for JournalGraph<'data> {
    fn cleanup_blank_predicates(&mut self) -> Result<()> {
        self.transaction
            .remove_where(|quad| quad.predicate.is_blank_node());
        Ok(())
    }

    fn quads<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
    ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a, 'data> {
        StagedDataset {
            source: &self.transaction,
        }
        .default_refs(subject, predicate, object)
        .map(|quad| Ok(quad.into_owned()))
    }

    #[cfg(test)]
    fn query(&self, query: PreparedSparqlQuery) -> Result<QueryResults<'_>> {
        Ok(query
            .on_queryable_dataset(StagedDataset {
                source: &self.transaction,
            })
            .execute()?)
    }

    fn inference_heads(
        &self,
        query: PreparedSparqlQuery,
    ) -> Result<oxigraph::sparql::QueryTripleIter<'_, GeneralizedTriple>> {
        Ok(query
            .on_queryable_dataset(StagedDataset {
                source: &self.transaction,
            })
            .execute_inference_construct()?)
    }

    fn contains_destination(&self, quad: GeneralizedQuadRef<'_>) -> Result<bool> {
        Ok(GeneralizedDataset::contains(
            &self.transaction,
            GeneralizedQuadRef {
                graph_name: self.destination.as_ref(),
                ..quad
            },
        ))
    }

    fn contains(&self, quad: GeneralizedQuadRef<'_>) -> Result<bool> {
        Ok(self
            .transaction
            .quads_for_pattern(
                Some(quad.subject),
                Some(quad.predicate),
                Some(quad.object),
                None,
            )
            .next()
            .is_some())
    }

    fn insert(&mut self, quad: GeneralizedQuad) -> Result<()> {
        self.transaction.insert(GeneralizedQuadRef {
            graph_name: self.destination.as_ref(),
            ..quad.as_ref()
        });
        Ok(())
    }
}

pub(super) fn expand(
    source: &mut GeneralizedDataset,
    mode: &str,
    destination: GraphName,
    request: InferenceRequest,
) -> Result<InferenceOutcome> {
    let mut graph = JournalGraph::new(source, destination);
    let (_, outcome) = expand_configured(&mut graph, mode, request)?;
    graph.commit();
    Ok(outcome)
}
