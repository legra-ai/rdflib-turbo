//! A read-only staged view over a dataset and its pending journal.
use oxigraph::model::*;
use oxrdf::GeneralizedDataset;

/// Read-only virtual default graph over the transaction's existing dictionary.
#[derive(Clone, Copy)]
pub(super) struct StagedDataset<'a> {
    pub(super) source: &'a GeneralizedDataset,
}

impl<'a> StagedDataset<'a> {
    pub(super) fn default_refs(
        &self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
    ) -> impl Iterator<Item = GeneralizedQuadRef<'a>> + use<'a> {
        // bounded: one borrowed previous SPO; native unscoped indexes order graph last.
        let mut previous = None;
        self.source
            .quads_for_pattern(subject, predicate, object, None)
            .filter(move |quad| {
                let triple = GeneralizedTripleRef {
                    subject: quad.subject,
                    predicate: quad.predicate,
                    object: quad.object,
                };
                previous.replace(triple) != Some(triple)
            })
            .map(|quad| GeneralizedQuadRef {
                graph_name: GraphNameRef::DefaultGraph,
                ..quad
            })
    }

    pub(super) fn matching_refs(
        &self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> Box<dyn Iterator<Item = GeneralizedQuadRef<'a>> + 'a> {
        match graph {
            Some(GraphNameRef::DefaultGraph) => {
                Box::new(self.default_refs(subject, predicate, object))
            }
            Some(graph) => {
                Box::new(
                    self.source
                        .quads_for_pattern(subject, predicate, object, Some(graph)),
                )
            }
            None => Box::new(
                self.default_refs(subject, predicate, object).chain(
                    self.source
                        .quads_for_pattern(subject, predicate, object, None)
                        .filter(|quad| !quad.graph_name.is_default_graph()),
                ),
            ),
        }
    }
}
