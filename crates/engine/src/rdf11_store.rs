//! Explicit RDF 1.1 string identity for the upstream Store convenience API.
use oxigraph::model::*;
use oxigraph::store::Transaction;

use crate::graph_write::GraphWrite;
use crate::{Error, GraphRead, Result};

#[cfg(test)]
#[path = "rdf11_store_tests.rs"]
mod tests;

pub(crate) struct Rdf11Transaction<'read, 'store> {
    inner: &'read mut Transaction<'store>,
}

impl<'read, 'store> Rdf11Transaction<'read, 'store> {
    pub(crate) fn new(inner: &'read mut Transaction<'store>) -> Self {
        Self { inner }
    }

    fn physical_matches<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a, 'read, 'store> {
        self.inner
            .generalized_quads_for_pattern(subject, predicate, object, graph)
            .map(|quad| quad.map_err(|error| Error::Runtime(error.to_string())))
    }

    fn logical_quad(&self, mut quad: GeneralizedQuad) -> Result<Option<GeneralizedQuad>> {
        let plain = matches!(&quad.object, Term::Literal(literal)
            if literal.datatype() == oxrdf::vocab::xsd::STRING && !literal.has_explicit_datatype());
        if !plain {
            return Ok(Some(quad));
        }
        let Term::Literal(literal) = &quad.object else {
            unreachable!("plain object checked above")
        };
        quad.object = Literal::new_typed_literal(literal.value(), oxrdf::vocab::xsd::STRING).into();
        let present = match QuadRef::try_from(quad.as_ref()) {
            // Preserve the direct encoded membership lookup for ordinary RDF.
            Ok(strict) => self
                .inner
                .contains(strict)
                .map_err(|error| Error::Runtime(error.to_string()))?,
            Err(_) => self
                .inner
                .generalized_quads_for_pattern(
                    Some(quad.subject.as_ref()),
                    Some(quad.predicate.as_ref()),
                    Some(quad.object.as_ref()),
                    Some(quad.graph_name.as_ref()),
                )
                .next()
                .transpose()
                .map_err(|error| Error::Runtime(error.to_string()))?
                .is_some(),
        };
        Ok((!present).then_some(quad))
    }
}

impl<'read, 'store> GraphRead for Rdf11Transaction<'read, 'store> {
    fn matching_quads<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a, 'read, 'store> {
        let candidates: Box<dyn Iterator<Item = Result<GeneralizedQuad>> + 'a> = match object {
            Some(TermRef::Literal(literal)) if literal.datatype() == oxrdf::vocab::xsd::STRING => {
                // bounded: two physical encodings of one bound query term.
                let plain = Literal::new_simple_literal(literal.value());
                let typed = Literal::new_typed_literal(literal.value(), oxrdf::vocab::xsd::STRING);
                Box::new(
                    self.physical_matches(subject, predicate, Some(plain.as_ref().into()), graph)
                        .chain(self.physical_matches(
                            subject,
                            predicate,
                            Some(typed.as_ref().into()),
                            graph,
                        )),
                )
            }
            _ => Box::new(self.physical_matches(subject, predicate, object, graph)),
        };
        Box::new(candidates.filter_map(
            |quad| match quad.and_then(|quad| self.logical_quad(quad)) {
                Ok(None) => None,
                Ok(Some(quad)) => Some(Ok(quad)),
                Err(error) => Some(Err(error)),
            },
        ))
    }

    fn named_graphs(&self) -> impl Iterator<Item = Result<NamedOrBlankNode>> + '_ {
        self.inner
            .named_graphs()
            .map(|graph| graph.map_err(|error| Error::Runtime(error.to_string())))
    }

    fn contains_named_graph(&self, graph: &NamedOrBlankNode) -> Result<bool> {
        self.inner
            .contains_named_graph(graph.as_ref())
            .map_err(|error| Error::Runtime(error.to_string()))
    }
}

impl GraphWrite for Rdf11Transaction<'_, '_> {
    fn insert_quad(&mut self, quad: QuadRef<'_>) -> Result<bool> {
        if matches!(quad.object, TermRef::Literal(literal)
            if literal.datatype() == oxrdf::vocab::xsd::STRING)
            && self
                .matching_quads(
                    Some(quad.subject.into()),
                    Some(quad.predicate.into()),
                    Some(quad.object),
                    Some(quad.graph_name),
                )
                .next()
                .transpose()?
                .is_some()
        {
            return Ok(false);
        }
        self.inner.insert_quad(quad)
    }
}
