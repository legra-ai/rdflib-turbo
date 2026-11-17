//! The journal graph that stages inference output before publication.
use oxigraph::model::*;
use oxigraph::sparql::PreparedSparqlQuery;
#[cfg(test)]
use oxigraph::sparql::QueryResults;
use oxrdf::{GeneralizedDataset, GeneralizedTransaction};

use super::super::backend::Graph;
use super::super::{InferenceOutcome, InferenceRequest, expand_configured};
use crate::Result;

use super::dataset::StagedDataset;

pub(in crate::inference) struct JournalGraph<'a> {
    pub(super) transaction: GeneralizedTransaction<'a>,
    destination: GraphName,
}

impl<'a> JournalGraph<'a> {
    pub(in crate::inference) fn new(
        source: &'a mut GeneralizedDataset,
        destination: GraphName,
    ) -> Self {
        Self {
            transaction: source.transaction(),
            destination,
        }
    }

    pub(in crate::inference) fn inserted(&self) -> impl Iterator<Item = GeneralizedQuadRef<'_>> {
        self.transaction.inserted()
    }

    pub(in crate::inference) fn commit(self) {
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

pub(in crate::inference) fn expand(
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
