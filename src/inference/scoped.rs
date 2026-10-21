//! Read one graph while journaling consequences into an independent destination.

use oxigraph::model::{GraphName, NamedNodeRef, NamedOrBlankNodeRef, Quad, TermRef};
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults};
use oxigraph::store::Transaction;

use super::backend::Graph;
use crate::{Error, Result};

struct ScopedGraph<'a, 'store> {
    transaction: &'a mut Transaction<'store>,
    source: GraphName,
    destination: GraphName,
}

impl Graph for ScopedGraph<'_, '_> {
    fn contains_destination(&self, quad: &Quad) -> Result<bool> {
        self.transaction
            .contains(&Quad::new(
                quad.subject.clone(),
                quad.predicate.clone(),
                quad.object.clone(),
                self.destination.clone(),
            ))
            .map_err(|error| Error::Runtime(error.to_string()))
    }

    fn quads<'a>(
        &'a self,
        subject: Option<NamedOrBlankNodeRef<'a>>,
        predicate: Option<NamedNodeRef<'a>>,
        object: Option<TermRef<'a>>,
    ) -> impl Iterator<Item = Result<Quad>> + 'a {
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
            .quads(
                Some(quad.subject.as_ref()),
                Some(quad.predicate.as_ref()),
                Some(quad.object.as_ref()),
            )
            .next()
            .transpose()?
            .is_some()
        {
            return Ok(true);
        }
        self.transaction
            .quads_for_pattern(
                Some(quad.subject.as_ref()),
                Some(quad.predicate.as_ref()),
                Some(quad.object.as_ref()),
                Some(self.destination.as_ref()),
            )
            .next()
            .transpose()
            .map(|quad| quad.is_some())
            .map_err(|error| Error::Runtime(error.to_string()))
    }

    fn insert(&mut self, mut quad: Quad) {
        quad.graph_name = self.destination.clone();
        self.transaction.insert(&quad);
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
