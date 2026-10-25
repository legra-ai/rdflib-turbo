//! The reasoner reads full-Term facts and inserts only derived consequences.
use oxigraph::model::*;
use oxigraph::sparql::PreparedSparqlQuery;
#[cfg(test)]
use oxigraph::sparql::QueryResults;
use oxigraph::store::Transaction;
use oxrdf::{GeneralizedDataset, GeneralizedTransaction};

use crate::{Error, Result};

pub(super) trait Graph {
    fn quads<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
    ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a, Self>;
    #[cfg(test)]
    fn query(&self, query: PreparedSparqlQuery) -> Result<QueryResults<'_>>;
    fn inference_heads(
        &self,
        query: PreparedSparqlQuery,
    ) -> Result<oxigraph::sparql::QueryTripleIter<'_, GeneralizedTriple>>;
    fn insert(&mut self, quad: GeneralizedQuad) -> Result<()>;
    fn cleanup_blank_predicates(&mut self) -> Result<()>;

    /// Direct reference publication checks the output scope, not the read union.
    fn contains_destination(&self, quad: GeneralizedQuadRef<'_>) -> Result<bool> {
        self.contains(quad)
    }

    fn contains(&self, quad: GeneralizedQuadRef<'_>) -> Result<bool> {
        self.quads(Some(quad.subject), Some(quad.predicate), Some(quad.object))
            .next()
            .transpose()
            .map(|quad| quad.is_some())
    }
}

impl<'data> Graph for GeneralizedTransaction<'data> {
    fn cleanup_blank_predicates(&mut self) -> Result<()> {
        self.remove_where(|quad| {
            quad.predicate.is_blank_node() && quad.graph_name.is_default_graph()
        });
        Ok(())
    }

    fn contains(&self, quad: GeneralizedQuadRef<'_>) -> Result<bool> {
        Ok(GeneralizedDataset::contains(
            self,
            GeneralizedQuadRef {
                graph_name: GraphNameRef::DefaultGraph,
                ..quad
            },
        ))
    }

    fn quads<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
    ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a, 'data> {
        self.quads_for_pattern(subject, predicate, object, Some(GraphNameRef::DefaultGraph))
            .map(|q| Ok(q.into_owned()))
    }

    #[cfg(test)]
    fn query(&self, query: PreparedSparqlQuery) -> Result<QueryResults<'_>> {
        Ok(query.on_queryable_dataset(&**self).execute()?)
    }

    fn inference_heads(
        &self,
        query: PreparedSparqlQuery,
    ) -> Result<oxigraph::sparql::QueryTripleIter<'_, GeneralizedTriple>> {
        Ok(query
            .on_queryable_dataset(&**self)
            .execute_inference_construct()?)
    }

    fn insert(&mut self, quad: GeneralizedQuad) -> Result<()> {
        GeneralizedTransaction::insert(self, &quad);
        Ok(())
    }
}

pub(crate) struct TransactionGraph<'a, 'store> {
    pub transaction: &'a mut Transaction<'store>,
    pub destination: GraphName,
}

impl<'owner, 'data> Graph for TransactionGraph<'owner, 'data> {
    fn cleanup_blank_predicates(&mut self) -> Result<()> {
        super::cleanup::transaction(self.transaction, None, &self.destination)
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
            .generalized_quads_for_pattern(subject, predicate, object, None)
            .map(|q| q.map_err(|e| Error::Runtime(e.to_string())))
    }

    #[cfg(test)]
    fn query(&self, mut query: PreparedSparqlQuery) -> Result<QueryResults<'_>> {
        query.dataset_mut().set_default_graph_as_union();
        Ok(query.on_transaction(self.transaction).execute()?)
    }

    fn inference_heads(
        &self,
        mut query: PreparedSparqlQuery,
    ) -> Result<oxigraph::sparql::QueryTripleIter<'_, GeneralizedTriple>> {
        query.dataset_mut().set_default_graph_as_union();
        Ok(query
            .on_transaction(self.transaction)
            .execute_inference_construct()?)
    }

    fn insert(&mut self, mut quad: GeneralizedQuad) -> Result<()> {
        quad.graph_name = self.destination.clone();
        self.transaction.insert_generalized(&quad);
        Ok(())
    }
}

#[cfg(test)]
#[path = "backend_boolean_tests.rs"]
mod boolean_tests;
