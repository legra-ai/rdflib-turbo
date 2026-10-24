//! Infer through the existing insertion journal and a borrowed logical union.

use oxigraph::model::*;
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults};
use oxrdf::dataset::InsertTransaction;

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
    source: &'a Dataset,
}

impl<'a> StagedDataset<'a> {
    fn default_refs(
        &self,
        subject: Option<NamedOrBlankNodeRef<'_>>,
        predicate: Option<NamedNodeRef<'_>>,
        object: Option<TermRef<'_>>,
    ) -> impl Iterator<Item = QuadRef<'a>> + use<'a> {
        // bounded: one borrowed previous SPO; native unscoped indexes order graph last.
        let mut previous = None;
        self.source
            .quads_for_pattern(subject, predicate, object, None)
            .filter(move |quad| {
                let triple = TripleRef::new(quad.subject, quad.predicate, quad.object);
                previous.replace(triple) != Some(triple)
            })
            .map(|quad| QuadRef {
                graph_name: GraphNameRef::DefaultGraph,
                ..quad
            })
    }

    fn matching_refs(
        &self,
        subject: Option<NamedOrBlankNodeRef<'_>>,
        predicate: Option<NamedNodeRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> Box<dyn Iterator<Item = QuadRef<'a>> + 'a> {
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
    transaction: InsertTransaction<'a>,
    destination: GraphName,
}

impl<'a> JournalGraph<'a> {
    pub(super) fn new(source: &'a mut Dataset, destination: GraphName) -> Self {
        Self {
            transaction: source.insert_transaction(),
            destination,
        }
    }

    pub(super) fn inserted(&self) -> impl Iterator<Item = QuadRef<'_>> {
        self.transaction.inserted()
    }

    pub(super) fn commit(self) {
        self.transaction.commit();
    }
}

impl<'data> Graph for JournalGraph<'data> {
    fn quads<'a>(
        &'a self,
        subject: Option<NamedOrBlankNodeRef<'_>>,
        predicate: Option<NamedNodeRef<'_>>,
        object: Option<TermRef<'_>>,
    ) -> impl Iterator<Item = Result<Quad>> + use<'a, 'data> {
        StagedDataset {
            source: &self.transaction,
        }
        .default_refs(subject, predicate, object)
        .map(|quad| Ok(quad.into_owned()))
    }

    fn query(&self, query: PreparedSparqlQuery) -> Result<QueryResults<'_>> {
        Ok(query
            .on_queryable_dataset(StagedDataset {
                source: &self.transaction,
            })
            .execute()?)
    }

    fn contains_destination(&self, quad: &Quad) -> Result<bool> {
        Ok(Dataset::contains(
            &self.transaction,
            QuadRef {
                graph_name: self.destination.as_ref(),
                ..quad.as_ref()
            },
        ))
    }

    fn contains(&self, quad: &Quad) -> Result<bool> {
        Ok(self
            .transaction
            .quads_for_pattern(
                Some(quad.subject.as_ref()),
                Some(quad.predicate.as_ref()),
                Some(quad.object.as_ref()),
                None,
            )
            .next()
            .is_some())
    }

    fn insert(&mut self, quad: Quad) -> Result<()> {
        self.transaction.insert(QuadRef {
            graph_name: self.destination.as_ref(),
            ..quad.as_ref()
        });
        Ok(())
    }
}

pub(super) fn expand(
    source: &mut Dataset,
    mode: &str,
    destination: GraphName,
    request: InferenceRequest,
) -> Result<InferenceOutcome> {
    let mut graph = JournalGraph::new(source, destination);
    let (_, outcome) = expand_configured(&mut graph, mode, request)?;
    graph.commit();
    Ok(outcome)
}
