//! Infer over a borrowed union, retaining only output absent from its destination.

use oxigraph::model::*;
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults};
use pyshacl_turbo_engine::GraphRead;

use super::backend::Graph;
use super::{InferenceOutcome, InferenceRequest, expand_configured};
use crate::Result;

#[path = "staged_query.rs"]
mod query;

struct StagedDataset<'a> {
    source: &'a Dataset,
    output: Output,
    destination: GraphName,
}

/// Disjoint output sets distinguish new facts from existing-source publications.
#[derive(Default)]
pub(super) struct Output {
    additions: Dataset,
    publications: Dataset,
}

impl Output {
    pub(super) fn iter(&self) -> impl Iterator<Item = QuadRef<'_>> {
        self.additions.iter().chain(self.publications.iter())
    }

    fn contains(&self, quad: QuadRef<'_>) -> bool {
        self.additions.contains(quad) || self.publications.contains(quad)
    }
}

impl<'a> StagedDataset<'a> {
    fn new(source: &'a Dataset, destination: GraphName) -> Self {
        Self {
            source,
            output: Output::default(),
            destination,
        }
    }

    fn source_contains_triple(&self, quad: QuadRef<'_>) -> bool {
        self.source
            .quads_for_pattern(
                Some(quad.subject),
                Some(quad.predicate),
                Some(quad.object),
                None,
            )
            .next()
            .is_some()
    }

    fn default_refs<'b>(
        &'b self,
        subject: Option<NamedOrBlankNodeRef<'_>>,
        predicate: Option<NamedNodeRef<'_>>,
        object: Option<TermRef<'_>>,
    ) -> impl Iterator<Item = QuadRef<'b>> + use<'a, 'b> {
        // bounded: one borrowed previous SPO; unscoped native indexes order graph last.
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

    fn matching_refs<'b>(
        &'b self,
        subject: Option<NamedOrBlankNodeRef<'_>>,
        predicate: Option<NamedNodeRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> Box<dyn Iterator<Item = QuadRef<'b>> + 'b> {
        let source: Box<dyn Iterator<Item = QuadRef<'b>> + 'b> = match graph {
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
        };
        Box::new(
            source.chain(
                self.output
                    .additions
                    .quads_for_pattern(subject, predicate, object, graph),
            ),
        )
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
        self.matching_refs(subject, predicate, object, graph)
            .map(|quad| Ok(quad.into_owned()))
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
        Ok(query.on_queryable_dataset(self).execute()?)
    }

    fn contains_destination(&self, quad: &Quad) -> Result<bool> {
        Ok(self.output_contains(quad)
            || self.output.contains(QuadRef {
                graph_name: GraphNameRef::DefaultGraph,
                ..quad.as_ref()
            }))
    }

    fn insert(&mut self, mut quad: Quad) {
        if !self.output_contains(&quad) {
            quad.graph_name = GraphName::DefaultGraph;
            if self.source_contains_triple(quad.as_ref()) {
                self.output.publications.insert(&quad);
            } else {
                self.output.additions.insert(&quad);
            }
        }
    }
}

pub(super) fn expand(
    source: &Dataset,
    mode: &str,
    destination: GraphName,
    request: InferenceRequest,
) -> Result<(Output, InferenceOutcome)> {
    let mut graph = StagedDataset::new(source, destination);
    let (_, outcome) = expand_configured(&mut graph, mode, request)?;
    Ok((graph.output, outcome))
}
