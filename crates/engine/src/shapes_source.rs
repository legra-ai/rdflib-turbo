//! Owned indexed shapes over one pinned native relation, without a base copy.

use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::*;
use oxigraph::store::{ReadSnapshot, Store};
use std::sync::Arc;

use crate::{Error, GraphRead, Result, ScopedGraph, UnionGraph};

/// A compiled shapes source retaining its actual immutable storage version.
///
/// A selected physical context is exposed as the sole logical default graph.
/// An absent scope exposes the dataset union as default and retains its named
/// contexts, including empty graphs. Neither form copies or reindexes rows.
#[derive(Clone)]
pub struct IndexedShapesSource {
    snapshot: Arc<ReadSnapshot>,
    scope: Option<GraphName>,
}

impl IndexedShapesSource {
    /// Own an existing committed read version and its exact logical scope.
    pub fn new(snapshot: ReadSnapshot, scope: Option<GraphName>) -> Self {
        Self {
            snapshot: Arc::new(snapshot),
            scope,
        }
    }

    /// Parse one syntax graph directly into its owned indexed source.
    pub fn parse_graph(text: &str, format: RdfFormat, base_iri: Option<&str>) -> Result<Self> {
        Self::parse_with(
            text,
            RdfParser::from_format(format).without_named_graphs(),
            base_iri,
            Some(GraphName::DefaultGraph),
        )
    }

    /// Stream a syntax parser directly into the retained indexed relation.
    ///
    /// Parsing never builds an intermediate syntax dataset or quad collection.
    /// The syntax parser keeps its own strict term-domain validation; broader
    /// programmatic term acceptance belongs to the typed shapes-input boundary.
    pub fn parse(
        text: &str,
        format: RdfFormat,
        base_iri: Option<&str>,
        scope: Option<GraphName>,
    ) -> Result<Self> {
        Self::parse_with(text, RdfParser::from_format(format), base_iri, scope)
    }

    fn parse_with(
        text: &str,
        mut parser: RdfParser,
        base_iri: Option<&str>,
        scope: Option<GraphName>,
    ) -> Result<Self> {
        if let Some(base) = base_iri {
            parser = parser
                .with_base_iri(base)
                .map_err(|error| Error::Invalid(format!("Invalid base IRI: {error}")))?;
        }
        let store = Store::new().map_err(|error| Error::Runtime(error.to_string()))?;
        let mut transaction = store
            .start_transaction()
            .map_err(|error| Error::Runtime(error.to_string()))?;
        for quad in parser.for_slice(text.as_bytes()) {
            let quad = quad.map_err(|error| Error::Invalid(format!("RDF parse error: {error}")))?;
            transaction.insert(&quad);
        }
        transaction
            .commit()
            .map_err(|error| Error::Runtime(error.to_string()))?;
        Ok(Self::new(store.read_snapshot(), scope))
    }
}

impl GraphRead for IndexedShapesSource {
    fn matching_quads<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a> {
        match &self.scope {
            Some(scope) => ScopedGraph::matching_source(
                self.snapshot.as_ref(),
                scope.as_ref(),
                subject,
                predicate,
                object,
                graph,
            ),
            None => UnionGraph::matching_source(
                self.snapshot.as_ref(),
                subject,
                predicate,
                object,
                graph,
            ),
        }
    }

    fn named_graphs(&self) -> impl Iterator<Item = Result<NamedOrBlankNode>> + '_ {
        let names: Box<dyn Iterator<Item = Result<NamedOrBlankNode>> + '_> = match &self.scope {
            Some(_) => Box::new(std::iter::empty()),
            None => Box::new(GraphRead::named_graphs(self.snapshot.as_ref())),
        };
        names
    }

    fn contains_named_graph(&self, graph: &NamedOrBlankNode) -> Result<bool> {
        match &self.scope {
            Some(_) => Ok(false),
            None => GraphRead::contains_named_graph(self.snapshot.as_ref(), graph),
        }
    }
}
