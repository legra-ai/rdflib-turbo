//! Read one graph while journaling consequences into an independent destination.

use oxigraph::model::{GeneralizedQuad, GeneralizedQuadRef, GeneralizedTriple, GraphName, TermRef};
use oxigraph::sparql::PreparedSparqlQuery;
#[cfg(test)]
use oxigraph::sparql::QueryResults;
use oxigraph::store::Transaction;

use super::backend::Graph;
use crate::{Error, Result};

struct ScopedGraph<'a, 'store> {
    transaction: &'a mut Transaction<'store>,
    source: GraphName,
    destination: GraphName,
}

impl<'owner, 'data> Graph for ScopedGraph<'owner, 'data> {
    fn cleanup_blank_predicates(&mut self) -> Result<()> {
        super::cleanup::transaction(self.transaction, Some(&self.source), &self.destination)
    }

    fn contains_destination(&self, quad: GeneralizedQuadRef<'_>) -> Result<bool> {
        self.transaction
            .contains_generalized(GeneralizedQuadRef {
                graph_name: self.destination.as_ref(),
                ..quad
            })
            .map_err(|error| Error::Runtime(error.to_string()))
    }

    fn quads<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
    ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a, 'owner, 'data> {
        self.transaction
            .generalized_quads_for_pattern(subject, predicate, object, Some(self.source.as_ref()))
            .map(|quad| quad.map_err(|error| Error::Runtime(error.to_string())))
    }

    #[cfg(test)]
    fn query(&self, mut query: PreparedSparqlQuery) -> Result<QueryResults<'_>> {
        // bounded: exactly one explicitly selected source graph.
        query
            .dataset_mut()
            .set_default_graph(vec![self.source.clone()]);
        Ok(query.on_transaction(self.transaction).execute()?)
    }

    fn inference_heads(
        &self,
        mut query: PreparedSparqlQuery,
    ) -> Result<oxigraph::sparql::QueryTripleIter<'_, GeneralizedTriple>> {
        // bounded: one exact named/blank/default source, independent of destination.
        query
            .dataset_mut()
            .set_default_graph(vec![self.source.clone()]);
        Ok(query
            .on_transaction(self.transaction)
            .execute_inference_construct()?)
    }

    fn contains(&self, quad: GeneralizedQuadRef<'_>) -> Result<bool> {
        if self
            .transaction
            .contains_generalized(GeneralizedQuadRef {
                graph_name: self.source.as_ref(),
                ..quad
            })
            .map_err(|error| Error::Runtime(error.to_string()))?
        {
            return Ok(true);
        }
        self.contains_destination(quad)
    }

    fn insert(&mut self, mut quad: GeneralizedQuad) -> Result<()> {
        quad.graph_name = self.destination.clone();
        self.transaction.insert_generalized(&quad);
        Ok(())
    }
}

/// Expand directly on a scoped transaction view, without copying its base.
pub(crate) fn expand(
    transaction: &mut Transaction<'_>,
    mode: &str,
    source: Option<GraphName>,
    destination: GraphName,
    request: super::InferenceRequest,
) -> Result<super::InferenceOutcome> {
    let Some(source) = source else {
        return super::expand_transaction(transaction, mode, destination, request);
    };
    super::expand_configured(
        &mut ScopedGraph {
            transaction,
            source,
            destination,
        },
        mode,
        request,
    )
    .map(|(_, outcome)| outcome)
}

#[cfg(test)]
#[path = "scoped_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "scoped_difference_tests.rs"]
mod difference_tests;

#[cfg(test)]
#[path = "scoped_boolean_tests.rs"]
mod boolean_tests;

#[cfg(test)]
#[path = "scoped_generalized_rollback_tests.rs"]
mod generalized_rollback_tests;

#[cfg(test)]
#[path = "scoped_term_tests.rs"]
mod term_tests;

#[cfg(test)]
#[path = "scoped_pending_term_tests.rs"]
mod pending_term_tests;

#[cfg(test)]
#[path = "scoped_procedural_term_tests.rs"]
mod procedural_term_tests;

#[cfg(test)]
#[path = "scoped_diagnostic_term_tests.rs"]
mod diagnostic_term_tests;

#[cfg(test)]
#[path = "scoped_head_term_tests.rs"]
mod head_term_tests;

#[cfg(test)]
#[path = "scoped_literal_value_tests.rs"]
mod literal_value_tests;
