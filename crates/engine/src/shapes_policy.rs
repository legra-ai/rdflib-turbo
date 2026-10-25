//! Compiler-only boolean projection over the original indexed source.
use oxigraph::model::*;

use crate::graph::{SH, XSD, iri};
use crate::{GraphRead, Result};

#[cfg(test)]
#[path = "shapes_policy_tests.rs"]
mod tests;

pub(crate) struct ShapesPolicy<'a, G: GraphRead + ?Sized> {
    source: &'a G,
    numeric_booleans: bool,
}

impl<'a, G: GraphRead + ?Sized> ShapesPolicy<'a, G> {
    pub fn new(source: &'a G, numeric_booleans: bool) -> Self {
        Self {
            source,
            numeric_booleans,
        }
    }

    fn rewrite(&self, quad: &GeneralizedQuad) -> bool {
        !self.numeric_booleans
            && [
                "closed",
                "uniqueLang",
                "deactivated",
                "qualifiedValueShapesDisjoint",
                "optional",
            ]
            .iter()
            .any(|name| quad.predicate == Term::from(iri(SH, name)))
            && matches!(&quad.object, Term::Literal(value)
                if value.datatype() == iri(XSD, "boolean").as_ref() && value.value() == "1")
    }

    fn project(&self, mut quad: GeneralizedQuad) -> Result<Option<GeneralizedQuad>> {
        if !self.rewrite(&quad) {
            return Ok(Some(quad));
        }
        quad.object = Literal::from(false).into();
        // Prefer the existing canonical row when both physical spellings map
        // to it. This exact lookup retains no graph-sized duplicate set.
        let exists = self
            .source
            .matching_quads(
                Some(quad.subject.as_ref()),
                Some(quad.predicate.as_ref()),
                Some(quad.object.as_ref()),
                Some(quad.graph_name.as_ref()),
            )
            .next()
            .transpose()?
            .is_some();
        Ok((!exists).then_some(quad))
    }
}

impl<'source, G: GraphRead + ?Sized> GraphRead for ShapesPolicy<'source, G> {
    fn matching_quads<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a, 'source, G> {
        // bounded: one optional object and at most two indexed input streams.
        let expected = object.map(TermRef::into_owned);
        let candidates: Box<dyn Iterator<Item = Result<GeneralizedQuad>> + 'a> =
            if !self.numeric_booleans && object == Some(Literal::from(false).as_ref().into()) {
                let one = Literal::new_typed_literal("1", iri(XSD, "boolean"));
                Box::new(
                    self.source
                        .matching_quads(subject, predicate, object, graph)
                        .chain(self.source.matching_quads(
                            subject,
                            predicate,
                            Some(one.as_ref().into()),
                            graph,
                        )),
                )
            } else {
                Box::new(
                    self.source
                        .matching_quads(subject, predicate, object, graph),
                )
            };
        candidates.filter_map(move |quad| match quad.and_then(|quad| self.project(quad)) {
            Err(error) => Some(Err(error)),
            Ok(Some(quad)) if expected.as_ref().is_none_or(|value| value == &quad.object) => {
                Some(Ok(quad))
            }
            Ok(_) => None,
        })
    }

    fn named_graphs(&self) -> impl Iterator<Item = Result<NamedOrBlankNode>> + '_ {
        self.source.named_graphs()
    }

    fn contains_named_graph(&self, graph: &NamedOrBlankNode) -> Result<bool> {
        self.source.contains_named_graph(graph)
    }
}
