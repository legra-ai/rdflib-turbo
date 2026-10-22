//! The reasoner reads one logical graph and inserts only derived facts.
use oxigraph::model::*;
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults};
use oxigraph::store::Transaction;
use oxrdf::dataset::InsertTransaction;

use crate::{Error, Result};

pub(super) trait Graph {
    fn quads<'a>(
        &'a self,
        subject: Option<NamedOrBlankNodeRef<'a>>,
        predicate: Option<NamedNodeRef<'a>>,
        object: Option<TermRef<'a>>,
    ) -> impl Iterator<Item = Result<Quad>> + 'a;
    fn query(&self, query: PreparedSparqlQuery) -> Result<QueryResults<'_>>;
    fn insert(&mut self, quad: Quad);

    /// Insert a distinct round result already proved absent from the read snapshot.
    fn insert_derived(&mut self, quad: Quad) {
        self.insert(quad);
    }

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

impl Graph for InsertTransaction<'_> {
    fn quads<'a>(
        &'a self,
        subject: Option<NamedOrBlankNodeRef<'a>>,
        predicate: Option<NamedNodeRef<'a>>,
        object: Option<TermRef<'a>>,
    ) -> impl Iterator<Item = Result<Quad>> + 'a {
        self.quads_for_pattern(subject, predicate, object, Some(GraphNameRef::DefaultGraph))
            .map(|q| Ok(q.into_owned()))
    }

    fn query(&self, query: PreparedSparqlQuery) -> Result<QueryResults<'_>> {
        Ok(query.on_queryable_dataset(&**self).execute()?)
    }

    fn insert(&mut self, quad: Quad) {
        InsertTransaction::insert(self, &quad);
    }
}

pub(crate) struct TransactionGraph<'a, 'store> {
    pub transaction: &'a mut Transaction<'store>,
    pub destination: GraphName,
}

impl Graph for TransactionGraph<'_, '_> {
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
            .quads_for_pattern(subject, predicate, object, None)
            .map(|q| q.map_err(|e| Error::Runtime(e.to_string())))
    }

    fn query(&self, mut query: PreparedSparqlQuery) -> Result<QueryResults<'_>> {
        query.dataset_mut().set_default_graph_as_union();
        Ok(query.on_transaction(self.transaction).execute()?)
    }

    fn insert(&mut self, mut quad: Quad) {
        quad.graph_name = self.destination.clone();
        self.transaction.insert(&quad);
    }
}
