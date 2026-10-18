//! Select native graph-scoped indexes before decoding matched statements.
use super::*;
use std::iter::empty;

// bounded: four encoded filter terms, independent of the number of stored quads.
struct Pattern {
    subject: Option<InternedNamedOrBlankNode>,
    predicate: Option<InternedNamedNode>,
    object: Option<InternedTerm>,
    graph: Option<InternedGraphName>,
}
impl Pattern {
    fn new(
        data: &Dataset,
        subject: Option<NamedOrBlankNodeRef<'_>>,
        predicate: Option<NamedNodeRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> Option<Self> {
        Some(Self {
            subject: match subject {
                Some(s) => Some(data.encoded_named_or_blank_node(s)?),
                None => None,
            },
            predicate: match predicate {
                Some(p) => Some(data.encoded_named_node(p)?),
                None => None,
            },
            object: match object {
                Some(o) => Some(data.encoded_term(o)?),
                None => None,
            },
            graph: match graph {
                Some(g) => Some(data.encoded_graph_name(g)?),
                None => None,
            },
        })
    }
    fn iter<'a>(self, data: &'a Dataset) -> Box<dyn Iterator<Item = QuadRef<'a>> + 'a> {
        // Decoded filter references borrow only the Dataset interner, not this pattern.
        let subject = self.subject.as_ref().map(|s| s.decode_from(&data.interner));
        let predicate = self.predicate.map(|p| p.decode_from(&data.interner));
        let object = self.object.as_ref().map(|o| o.decode_from(&data.interner));
        if let Some(graph) = self.graph {
            let view = GraphView {
                dataset: data,
                graph_name: graph,
            };
            let graph_name = graph.decode_from(&data.interner);
            return Box::new(
                view.triples_for_pattern(subject, predicate, object)
                    .map(move |t| QuadRef::new(t.subject, t.predicate, t.object, graph_name)),
            );
        }
        if let Some(subject) = subject {
            Box::new(data.quads_for_subject(subject).filter(move |q| {
                predicate.is_none_or(|p| p == q.predicate) && object.is_none_or(|o| o == q.object)
            }))
        } else if let Some(object) = object {
            Box::new(
                data.quads_for_object(object)
                    .filter(move |q| predicate.is_none_or(|p| p == q.predicate)),
            )
        } else if let Some(predicate) = predicate {
            Box::new(data.quads_for_predicate(predicate))
        } else {
            Box::new(data.iter())
        }
    }
}
impl Dataset {
    /// Retrieves quads with a filter on each component, independent of filter lifetimes.
    pub fn quads_for_pattern<'a, 'b>(
        &'a self,
        subject: Option<NamedOrBlankNodeRef<'b>>,
        predicate: Option<NamedNodeRef<'b>>,
        object: Option<TermRef<'b>>,
        graph_name: Option<GraphNameRef<'b>>,
    ) -> impl Iterator<Item = QuadRef<'a>> + use<'a> {
        let iter: Box<dyn Iterator<Item = QuadRef<'a>> + 'a> =
            match Pattern::new(self, subject, predicate, object, graph_name) {
                Some(pattern) => pattern.iter(self),
                None => Box::new(empty()),
            };
        iter
    }
}
