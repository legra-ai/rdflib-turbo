//! Generalized in-memory reads retain the full stored term domain.
use oxigraph::model::*;
use spareval::{InternalQuad, TermCow};

use crate::{GraphRead, Result};

impl GraphRead for GeneralizedDataset {
    fn matching_quads<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a> {
        self.quads_for_pattern(subject, predicate, object, graph)
            .map(|quad| Ok(quad.into_owned()))
    }

    fn matching_query_quads<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = Result<InternalQuad<TermCow<'a>>>> + use<'a> {
        self.quads_for_pattern(subject, predicate, object, graph)
            .map(|quad| Ok(crate::query_graph::borrowed_quad(quad)))
    }

    fn named_graphs(&self) -> impl Iterator<Item = Result<NamedOrBlankNode>> + '_ {
        GeneralizedDataset::named_graphs(self).map(|name| Ok(name.into_owned()))
    }

    fn contains_named_graph(&self, graph: &NamedOrBlankNode) -> Result<bool> {
        Ok(self
            .quads_for_pattern(None, None, None, Some(graph.as_ref().into()))
            .next()
            .is_some())
    }
}
