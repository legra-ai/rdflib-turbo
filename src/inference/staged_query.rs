//! Keep stored terms borrowed through SPARQL joins over the staged union.
use std::convert::Infallible;

use oxigraph::model::*;
use oxrdf::GeneralizedDataset;
use spareval::{InternalQuad, QueryableDataset};

use super::StagedDataset;

type QueryTerm<'a> = <&'a GeneralizedDataset as QueryableDataset<'a>>::InternalTerm;
type Rows<'a> = Box<dyn Iterator<Item = Result<InternalQuad<QueryTerm<'a>>, Infallible>> + 'a>;

#[cfg(test)]
#[path = "staged_query_tests.rs"]
mod tests;

impl<'a> QueryableDataset<'a> for StagedDataset<'a> {
    type InternalTerm = QueryTerm<'a>;
    type Error = Infallible;

    fn internal_quads_for_pattern(
        &self,
        subject: Option<&Self::InternalTerm>,
        predicate: Option<&Self::InternalTerm>,
        object: Option<&Self::InternalTerm>,
        graph_name: Option<Option<&Self::InternalTerm>>,
    ) -> impl Iterator<Item = Result<InternalQuad<QueryTerm<'a>>, Infallible>> + use<'a> {
        let subject = subject.map(TermRef::from);
        let predicate = predicate.map(TermRef::from);
        let graph = match graph_name {
            Some(Some(term)) => match TermRef::from(term) {
                TermRef::NamedNode(node) => Some(GraphNameRef::from(node)),
                TermRef::BlankNode(node) => Some(GraphNameRef::from(node)),
                _ => return Box::new(std::iter::empty()) as Rows<'a>,
            },
            Some(None) => Some(GraphNameRef::DefaultGraph),
            None => None,
        };
        let object = object.map(TermRef::from);
        let rows: Box<dyn Iterator<Item = GeneralizedQuadRef<'a>> + 'a> = if graph_name.is_none() {
            Box::new(
                self.source
                    .quads_for_pattern(subject, predicate, object, None)
                    .filter(|quad| !quad.graph_name.is_default_graph()),
            )
        } else {
            self.matching_refs(subject, predicate, object, graph)
        };
        Box::new(rows.map(|quad| {
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
        }))
    }

    fn internalize_term(&self, term: Term) -> Result<QueryTerm<'a>, Infallible> {
        let source: &'a GeneralizedDataset = self.source;
        source.internalize_term(term)
    }

    fn externalize_term(&self, term: QueryTerm<'a>) -> Result<Term, Infallible> {
        let source: &'a GeneralizedDataset = self.source;
        source.externalize_term(term)
    }
}
