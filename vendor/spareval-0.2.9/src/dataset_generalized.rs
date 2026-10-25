//! Full-Term indexed query reads, borrowing the generalized owner's dictionary.
use super::{InternalQuad, QueryableDataset, TermCow};
use oxrdf::{GeneralizedDataset, GeneralizedQuadRef, GraphNameRef, Term, TermRef};
use std::convert::Infallible;
use std::iter::empty;

impl<'a> QueryableDataset<'a> for &'a GeneralizedDataset {
    type InternalTerm = TermCow<'a>;
    type Error = Infallible;

    fn internal_quads_for_pattern(
        &self,
        subject: Option<&TermCow<'a>>,
        predicate: Option<&TermCow<'a>>,
        object: Option<&TermCow<'a>>,
        graph_name: Option<Option<&TermCow<'a>>>,
    ) -> impl Iterator<Item = Result<InternalQuad<TermCow<'a>>, Infallible>> + use<'a> {
        #[expect(clippy::unnecessary_wraps)]
        fn row(quad: GeneralizedQuadRef<'_>) -> Result<InternalQuad<TermCow<'_>>, Infallible> {
            Ok(InternalQuad {
                subject: quad.subject.into(),
                predicate: quad.predicate.into(),
                object: quad.object.into(),
                graph_name: match quad.graph_name {
                    GraphNameRef::NamedNode(node) => Some(TermRef::from(node).into()),
                    GraphNameRef::BlankNode(node) => Some(TermRef::from(node).into()),
                    GraphNameRef::DefaultGraph => None,
                },
            })
        }
        let graph_name = if let Some(graph_name) = graph_name {
            Some(if let Some(graph_name) = graph_name {
                match TermRef::from(graph_name) {
                    TermRef::NamedNode(node) => node.into(),
                    TermRef::BlankNode(node) => node.into(),
                    TermRef::Literal(_) => {
                        let empty: Box<dyn Iterator<Item = Result<_, _>>> = Box::new(empty());
                        return empty;
                    }
                    #[cfg(feature = "sparql-12")]
                    TermRef::Triple(_) => return Box::new(empty()),
                }
            } else {
                GraphNameRef::DefaultGraph
            })
        } else {
            None
        };
        let named_only = graph_name.is_none();
        Box::new(
            (*self)
                .quads_for_pattern(
                    subject.map(TermRef::from),
                    predicate.map(TermRef::from),
                    object.map(TermRef::from),
                    graph_name,
                )
                .filter(move |quad| !named_only || !quad.graph_name.is_default_graph())
                .map(row),
        )
    }

    fn internal_named_graphs(
        &self,
    ) -> impl Iterator<Item = Result<TermCow<'a>, Infallible>> + use<'a> {
        (*self)
            .named_graphs()
            .map(|name| Ok(TermCow::Borrowed(name.into())))
    }

    fn contains_internal_graph_name(&self, graph: &TermCow<'a>) -> Result<bool, Infallible> {
        let graph = match TermRef::from(graph) {
            TermRef::NamedNode(node) => GraphNameRef::from(node),
            TermRef::BlankNode(node) => GraphNameRef::from(node),
            TermRef::Literal(_) => return Ok(false),
            #[cfg(feature = "sparql-12")]
            TermRef::Triple(_) => return Ok(false),
        };
        Ok((*self)
            .quads_for_pattern(None, None, None, Some(graph))
            .next()
            .is_some())
    }

    fn internalize_term(&self, term: Term) -> Result<TermCow<'a>, Infallible> {
        Ok(term.into())
    }

    fn externalize_term(&self, term: TermCow<'a>) -> Result<Term, Infallible> {
        Ok(term.into())
    }
}
