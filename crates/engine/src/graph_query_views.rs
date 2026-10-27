//! Query-only borrowed/owned row composition; physical GraphRead is unchanged.
use super::*;

type QueryMatches<'a> = Box<dyn Iterator<Item = Result<InternalQuad<TermCow<'a>>>> + 'a>;

impl<G: GraphRead + ?Sized> UnionGraph<'_, G> {
    fn query_default<'a>(
        source: &'a G,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
    ) -> impl Iterator<Item = Result<InternalQuad<TermCow<'a>>>> + use<'a, G> {
        source
            .matching_query_quads(subject, predicate, object, None)
            .filter_map(|quad| match quad {
                Err(error) => Some(Err(error)),
                Ok(mut quad) => match Self::query_representative(source, &quad) {
                    Err(error) => Some(Err(error)),
                    Ok(false) => None,
                    Ok(true) => {
                        quad.graph_name = None;
                        Some(Ok(quad))
                    }
                },
            })
    }

    fn query_representative(source: &G, quad: &InternalQuad<TermCow<'_>>) -> Result<bool> {
        let first = source
            .matching_query_quads(
                Some(TermRef::from(&quad.subject)),
                Some(TermRef::from(&quad.predicate)),
                Some(TermRef::from(&quad.object)),
                None,
            )
            .next()
            .transpose()?
            .ok_or_else(|| Error::Runtime("Graph changed during union read".into()))?;
        Ok(first.graph_name == quad.graph_name)
    }

    pub(super) fn query_source<'a>(
        source: &'a G,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> QueryMatches<'a> {
        match graph {
            Some(GraphNameRef::DefaultGraph) => {
                Box::new(Self::query_default(source, subject, predicate, object))
            }
            Some(_) => Box::new(source.matching_query_quads(subject, predicate, object, graph)),
            None => Box::new(
                Self::query_default(source, subject, predicate, object).chain(
                    source
                        .matching_query_quads(subject, predicate, object, None)
                        .filter(|quad| match quad {
                            Ok(quad) => quad.graph_name.is_some(),
                            Err(_) => true,
                        }),
                ),
            ),
        }
    }
}

impl<G: GraphRead + ?Sized, S: GraphRead + ?Sized> OverlayGraph<'_, G, S> {
    fn query_overlay<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
    ) -> impl Iterator<Item = Result<InternalQuad<TermCow<'a>>>> + use<'a, G, S> {
        self.shapes
            .matching_query_quads(subject, predicate, object, Some(GraphNameRef::DefaultGraph))
            .map(|quad| {
                quad.map(|mut quad| {
                    quad.graph_name = Some(TermCow::Borrowed(match self.graph.as_ref() {
                        GraphNameRef::NamedNode(node) => node.into(),
                        GraphNameRef::BlankNode(node) => node.into(),
                        GraphNameRef::DefaultGraph => {
                            unreachable!("overlay reserves a named graph")
                        }
                    }));
                    quad
                })
            })
    }

    pub(super) fn query_source<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> QueryMatches<'a> {
        match graph {
            Some(graph) if graph == self.graph.as_ref() => {
                Box::new(self.query_overlay(subject, predicate, object))
            }
            Some(_) => Box::new(
                self.source
                    .matching_query_quads(subject, predicate, object, graph),
            ),
            None => Box::new(
                self.source
                    .matching_query_quads(subject, predicate, object, None)
                    .chain(self.query_overlay(subject, predicate, object)),
            ),
        }
    }
}

impl<G: GraphRead + ?Sized> ScopedGraph<'_, G> {
    pub(super) fn query_source<'a>(
        source: &'a G,
        physical_graph: GraphNameRef<'_>,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> QueryMatches<'a> {
        if graph.is_some_and(|graph| !graph.is_default_graph()) {
            Box::new(std::iter::empty())
        } else {
            Box::new(
                source
                    .matching_query_quads(subject, predicate, object, Some(physical_graph))
                    .map(|quad| {
                        quad.map(|mut quad| {
                            quad.graph_name = None;
                            quad
                        })
                    }),
            )
        }
    }
}
