//! Infer over a borrowed union, retaining only output absent from its destination.

use oxigraph::model::*;
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults};
use pyshacl_turbo_engine::{GraphRead, QueryGraph, UnionGraph};

use super::backend::Graph;
use super::{InferenceOutcome, InferenceRequest, expand_configured};
use crate::Result;

struct StagedDataset<'a> {
    source: &'a Dataset,
    union: UnionGraph<'a, Dataset>,
    additions: Dataset,
    destination: GraphName,
}

impl<'a> StagedDataset<'a> {
    fn new(source: &'a Dataset, destination: GraphName) -> Self {
        Self {
            source,
            union: UnionGraph::new(source),
            additions: Dataset::new(),
            destination,
        }
    }

    fn source_contains_triple(&self, quad: &Quad) -> bool {
        self.source
            .quads_for_subject(quad.subject.as_ref())
            .any(|stored| {
                stored.predicate == quad.predicate.as_ref() && stored.object == quad.object.as_ref()
            })
    }

    fn output_contains(&self, quad: &Quad) -> bool {
        self.source.contains(QuadRef {
            graph_name: self.destination.as_ref(),
            ..quad.as_ref()
        })
    }
}

impl<'source> GraphRead for StagedDataset<'source> {
    fn matching_quads<'a>(
        &'a self,
        subject: Option<NamedOrBlankNodeRef<'_>>,
        predicate: Option<NamedNodeRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = pyshacl_turbo_engine::Result<Quad>> + use<'a, 'source> {
        self.union
            .matching_quads(subject, predicate, object, graph)
            .chain(
                self.additions
                    .matching_quads(subject, predicate, object, graph)
                    .filter_map(|quad| match quad {
                        Ok(quad) if self.source_contains_triple(&quad) => None,
                        other => Some(other),
                    }),
            )
    }

    fn named_graphs(
        &self,
    ) -> impl Iterator<Item = pyshacl_turbo_engine::Result<NamedOrBlankNode>> + '_ {
        self.source.named_graphs()
    }

    fn contains_named_graph(&self, graph: &NamedOrBlankNode) -> pyshacl_turbo_engine::Result<bool> {
        self.source.contains_named_graph(graph)
    }
}

impl Graph for StagedDataset<'_> {
    fn quads<'a>(
        &'a self,
        subject: Option<NamedOrBlankNodeRef<'a>>,
        predicate: Option<NamedNodeRef<'a>>,
        object: Option<TermRef<'a>>,
    ) -> impl Iterator<Item = Result<Quad>> + 'a {
        self.matching_quads(subject, predicate, object, Some(GraphNameRef::DefaultGraph))
            .map(|quad| quad.map_err(Into::into))
    }

    fn query(&self, query: PreparedSparqlQuery) -> Result<QueryResults<'_>> {
        Ok(query
            .on_queryable_dataset(QueryGraph::new(self))
            .execute()?)
    }

    fn contains_destination(&self, quad: &Quad) -> Result<bool> {
        Ok(self.output_contains(quad)
            || self.additions.contains(QuadRef {
                graph_name: GraphNameRef::DefaultGraph,
                ..quad.as_ref()
            }))
    }

    fn insert(&mut self, mut quad: Quad) {
        if !self.output_contains(&quad) {
            quad.graph_name = GraphName::DefaultGraph;
            self.additions.insert(&quad);
        }
    }
}

pub(super) fn expand(
    source: &Dataset,
    mode: &str,
    destination: GraphName,
    request: InferenceRequest,
) -> Result<(Dataset, InferenceOutcome)> {
    let mut graph = StagedDataset::new(source, destination);
    let (_, outcome) = expand_configured(&mut graph, mode, request)?;
    Ok((graph.additions, outcome))
}
