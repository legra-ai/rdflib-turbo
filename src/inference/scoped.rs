//! Read one graph while journaling consequences into an independent destination.

use oxigraph::model::{GraphName, NamedNodeRef, NamedOrBlankNodeRef, Quad, QuadRef, TermRef};
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults};
use oxigraph::store::Transaction;

use super::backend::Graph;
use crate::{Error, Result};

struct ScopedGraph<'a, 'store> {
    transaction: &'a mut Transaction<'store>,
    source: GraphName,
    destination: GraphName,
}

impl<'graph, 'store> Graph for ScopedGraph<'graph, 'store> {
    fn contains_destination(&self, quad: &Quad) -> Result<bool> {
        self.transaction
            .contains(QuadRef {
                graph_name: self.destination.as_ref(),
                ..quad.as_ref()
            })
            .map_err(|error| Error::Runtime(error.to_string()))
    }

    fn quads<'a>(
        &'a self,
        subject: Option<NamedOrBlankNodeRef<'_>>,
        predicate: Option<NamedNodeRef<'_>>,
        object: Option<TermRef<'_>>,
    ) -> impl Iterator<Item = Result<Quad>> + use<'a, 'graph, 'store> {
        self.transaction
            .quads_for_pattern(subject, predicate, object, Some(self.source.as_ref()))
            .map(|quad| quad.map_err(|error| Error::Runtime(error.to_string())))
    }

    fn query(&self, mut query: PreparedSparqlQuery) -> Result<QueryResults<'_>> {
        // bounded: exactly one explicitly selected source graph.
        query
            .dataset_mut()
            .set_default_graph(vec![self.source.clone()]);
        Ok(query.on_transaction(self.transaction).execute()?)
    }

    fn contains(&self, quad: &Quad) -> Result<bool> {
        if self
            .transaction
            .contains(QuadRef {
                graph_name: self.source.as_ref(),
                ..quad.as_ref()
            })
            .map_err(|error| Error::Runtime(error.to_string()))?
        {
            return Ok(true);
        }
        self.contains_destination(quad)
    }

    fn insert(&mut self, mut quad: Quad) -> Result<()> {
        quad.graph_name = self.destination.clone();
        self.transaction.insert(&quad);
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
