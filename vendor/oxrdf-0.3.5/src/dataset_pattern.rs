//! Select native graph-scoped indexes before decoding matched statements.
use super::*;
#[cfg(test)]
use std::cell::Cell;
use std::iter::empty;
#[cfg(test)]
#[path = "dataset_unscoped_work_tests.rs"]
mod work_tests;

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
    fn iter<'a>(
        self,
        data: &'a Dataset,
        #[cfg(test)] examined: Option<&'a Cell<usize>>,
    ) -> Box<dyn Iterator<Item = QuadRef<'a>> + 'a> {
        if let Some(graph) = self.graph {
            return self.scoped(data, graph);
        }
        if let (Some(subject), Some(predicate), Some(object)) =
            (self.subject, self.predicate, self.object.as_ref())
        {
            // Exact SPO membership spans only its graph placements. Starting at
            // the full prefix avoids decoding a shared subject's unrelated rows.
            let start = SpogKey::from_tuple((
                subject,
                predicate,
                object.clone(),
                InternedGraphName::first(),
            ));
            let object = self.object.expect("matched bound object");
            return Box::new(
                data.spog
                    .range(start..)
                    .take_while(move |q| {
                        #[cfg(test)]
                        if let Some(examined) = examined {
                            examined.set(examined.get() + 1);
                        }
                        let (s, p, o, _) = q.parts();
                        s == subject && *p == predicate && o == &object
                    })
                    .map(move |q| data.decode_spog(q.parts())),
            );
        }
        if let (Some(subject), Some(predicate), None) =
            (self.subject, self.predicate, self.object.as_ref())
        {
            // The existing SP prefix retains the legacy filtered row order.
            return Box::new(
                data.spog
                    .range(SpogKey::from_tuple((subject, predicate, InternedTerm::first(), InternedGraphName::first()))..SpogKey::from_tuple((subject, predicate.next(), InternedTerm::first(), InternedGraphName::first())))
                    .map(move |q| {
                        #[cfg(test)]
                        if let Some(examined) = examined {
                            examined.set(examined.get() + 1);
                        }
                        data.decode_spog(q.parts())
                    }),
            );
        }
        if let (Some(subject), None, Some(object)) =
            (self.subject, self.predicate, self.object.as_ref())
        {
            // The existing SO prefix retains the legacy filtered row order.
            return Box::new(
                data.ospg
                    .range(OspgKey::from_tuple((object.clone(), subject, InternedNamedNode::first(), InternedGraphName::first()))..OspgKey::from_tuple((object.clone(), subject.next(), InternedNamedNode::first(), InternedGraphName::first())))
                    .map(move |q| {
                        #[cfg(test)]
                        if let Some(examined) = examined {
                            examined.set(examined.get() + 1);
                        }
                        data.decode_spog(q.parts())
                    }),
            );
        }
        if let (None, Some(predicate), Some(object)) =
            (self.subject, self.predicate, self.object.as_ref())
        {
            // The existing PO prefix retains the legacy filtered row order.
            return Box::new(
                data.posg
                    .range(PosgKey::from_tuple((predicate, object.clone(), InternedNamedOrBlankNode::first(), InternedGraphName::first()))..PosgKey::from_tuple((predicate, object.next(), InternedNamedOrBlankNode::first(), InternedGraphName::first())))
                    .map(move |q| {
                        #[cfg(test)]
                        if let Some(examined) = examined {
                            examined.set(examined.get() + 1);
                        }
                        data.decode_spog(q.parts())
                    }),
            );
        }
        let predicate = self.predicate.map(|p| p.decode_from(&data.interner));
        if let Some(subject) = self.subject {
            let object = self.object.as_ref().map(|o| o.decode_from(&data.interner));
            Box::new(
                data.interned_quads_for_subject(&subject)
                    .map(move |q| {
                        #[cfg(test)]
                        if let Some(examined) = examined {
                            examined.set(examined.get() + 1);
                        }
                        data.decode_spog(q)
                    })
                    .filter(move |q| {
                        predicate.is_none_or(|p| p == q.predicate)
                            && object.is_none_or(|o| o == q.object)
                    }),
            )
        } else if let Some(object) = self.object {
            Box::new(
                data.interned_quads_for_object(&object)
                    .map(move |q| {
                        #[cfg(test)]
                        if let Some(examined) = examined {
                            examined.set(examined.get() + 1);
                        }
                        data.decode_spog(q)
                    })
                    .filter(move |q| predicate.is_none_or(|p| p == q.predicate)),
            )
        } else if let Some(predicate) = self.predicate {
            Box::new(data.interned_quads_for_predicate(predicate).map(move |q| {
                #[cfg(test)]
                if let Some(examined) = examined {
                    examined.set(examined.get() + 1);
                }
                data.decode_spog(q)
            }))
        } else {
            Box::new(data.iter())
        }
    }

    fn scoped<'a>(
        self,
        data: &'a Dataset,
        graph: InternedGraphName,
    ) -> Box<dyn Iterator<Item = QuadRef<'a>> + 'a> {
        let view = GraphView {
            dataset: data,
            graph_name: graph,
        };
        let graph_name = graph.decode_from(&data.interner);
        match (self.subject, self.predicate, self.object) {
            (Some(subject), Some(predicate), Some(object)) => {
                let record = QuadRecord::from_gspo((graph, subject, predicate, object));
                Box::new(
                    data.gspo
                        .get(&record)
                        .into_iter()
                        .map(move |q| data.decode_spog(q.parts())),
                )
            }
            (Some(subject), Some(predicate), None) => {
                let s = subject.decode_from(&data.interner);
                let p = predicate.decode_from(&data.interner);
                Self::quads(
                    graph_name,
                    view.objects_for_interned_subject_predicate(Some(subject), Some(predicate))
                        .map(move |o| TripleRef::new(s, p, o)),
                )
            }
            (Some(subject), None, Some(object)) => {
                let s = subject.decode_from(&data.interner);
                let o = object.decode_from(&data.interner);
                Self::quads(
                    graph_name,
                    view.predicates_for_interned_subject_object(Some(subject), Some(object))
                        .map(move |p| TripleRef::new(s, p, o)),
                )
            }
            (Some(subject), None, None) => {
                Self::quads(graph_name, view.triples_for_interned_subject(Some(subject)))
            }
            (None, Some(predicate), Some(object)) => {
                let p = predicate.decode_from(&data.interner);
                let o = object.decode_from(&data.interner);
                Self::quads(
                    graph_name,
                    view.subjects_for_interned_predicate_object(Some(predicate), Some(object))
                        .map(move |s| TripleRef::new(s, p, o)),
                )
            }
            (None, Some(predicate), None) => Self::quads(
                graph_name,
                view.triples_for_interned_predicate(Some(predicate)),
            ),
            (None, None, Some(object)) => {
                Self::quads(graph_name, view.triples_for_interned_object(Some(object)))
            }
            (None, None, None) => Self::quads(graph_name, view.iter()),
        }
    }

    fn quads<'a>(
        graph: GraphNameRef<'a>,
        triples: impl Iterator<Item = TripleRef<'a>> + 'a,
    ) -> Box<dyn Iterator<Item = QuadRef<'a>> + 'a> {
        Box::new(triples.map(move |t| QuadRef::new(t.subject, t.predicate, t.object, graph)))
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
                Some(pattern) => pattern.iter(
                    self,
                    #[cfg(test)]
                    None,
                ),
                None => Box::new(empty()),
            };
        iter
    }
}
