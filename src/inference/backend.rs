//! The reasoner reads one logical graph and inserts only derived facts.
use oxigraph::model::*;
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults};
use oxigraph::store::Transaction;
use oxrdf::dataset::InsertTransaction;

use crate::{Error, Result};

pub(super) trait Graph {
    fn quads<'a>(
        &'a self,
        subject: Option<NamedOrBlankNodeRef<'_>>,
        predicate: Option<NamedNodeRef<'_>>,
        object: Option<TermRef<'_>>,
    ) -> impl Iterator<Item = Result<Quad>> + use<'a, Self>;
    fn query(&self, query: PreparedSparqlQuery) -> Result<QueryResults<'_>>;
    fn insert(&mut self, quad: Quad) -> Result<()>;

    /// Direct reference publication checks the output scope, not the read union.
    fn contains_destination(&self, quad: &Quad) -> Result<bool> {
        self.contains(quad)
    }

    fn contains(&self, quad: &Quad) -> Result<bool> {
        self.quads(
            Some(quad.subject.as_ref()),
            Some(quad.predicate.as_ref()),
            Some(quad.object.as_ref()),
        )
        .next()
        .transpose()
        .map(|quad| quad.is_some())
    }
}

impl<'data> Graph for InsertTransaction<'data> {
    fn contains(&self, quad: &Quad) -> Result<bool> {
        Ok(Dataset::contains(
            self,
            QuadRef {
                graph_name: GraphNameRef::DefaultGraph,
                ..quad.as_ref()
            },
        ))
    }

    fn quads<'a>(
        &'a self,
        subject: Option<NamedOrBlankNodeRef<'_>>,
        predicate: Option<NamedNodeRef<'_>>,
        object: Option<TermRef<'_>>,
    ) -> impl Iterator<Item = Result<Quad>> + use<'a, 'data> {
        self.quads_for_pattern(subject, predicate, object, Some(GraphNameRef::DefaultGraph))
            .map(|q| Ok(q.into_owned()))
    }

    fn query(&self, query: PreparedSparqlQuery) -> Result<QueryResults<'_>> {
        Ok(query.on_queryable_dataset(&**self).execute()?)
    }

    fn insert(&mut self, quad: Quad) -> Result<()> {
        InsertTransaction::insert(self, &quad);
        Ok(())
    }
}

pub(crate) struct TransactionGraph<'a, 'store> {
    pub transaction: &'a mut Transaction<'store>,
    pub destination: GraphName,
}

impl<'graph, 'store> Graph for TransactionGraph<'graph, 'store> {
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
            .quads_for_pattern(subject, predicate, object, None)
            .map(|q| q.map_err(|e| Error::Runtime(e.to_string())))
    }

    fn query(&self, mut query: PreparedSparqlQuery) -> Result<QueryResults<'_>> {
        query.dataset_mut().set_default_graph_as_union();
        Ok(query.on_transaction(self.transaction).execute()?)
    }

    fn insert(&mut self, mut quad: Quad) -> Result<()> {
        quad.graph_name = self.destination.clone();
        self.transaction.insert(&quad);
        Ok(())
    }
}

#[cfg(test)]
#[path = "backend_boolean_tests.rs"]
mod boolean_tests;
