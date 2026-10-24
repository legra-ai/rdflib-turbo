//! SPARQL joins over ordinary triples and indexed generalized intermediate rows.

use oxigraph::model::*;
use spareval::{InternalQuad, QueryableDataset};

use super::{GENERALIZED_GRAPH, Generalized};
use crate::inference::backend::Graph;
use crate::{Error, Result};

pub(super) struct View<'a, G> {
    pub(super) data: &'a G,
    pub(super) extra: &'a Generalized,
}

impl<'a, G: Graph + 'a> QueryableDataset<'a> for View<'a, G> {
    type InternalTerm = Term;
    type Error = Error;

    fn internal_quads_for_pattern(
        &self,
        subject: Option<&Term>,
        predicate: Option<&Term>,
        object: Option<&Term>,
        graph_name: Option<Option<&Term>>,
    ) -> impl Iterator<Item = Result<InternalQuad<Term>>> + use<'a, G> {
        let default_graph = graph_name == Some(None);
        let generalized_graph = matches!(graph_name, Some(Some(Term::NamedNode(node))) if node.as_ref() == GENERALIZED_GRAPH);
        let data = self.data;
        let extra = self.extra;
        let pattern = [subject, predicate, object].map(|term| term.map(Term::as_ref));
        let standard_subject = subject.and_then(|term| match term {
            Term::NamedNode(node) => Some(node.as_ref().into()),
            Term::BlankNode(node) => Some(node.as_ref().into()),
            Term::Literal(_) => None,
        });
        let standard_predicate = predicate.and_then(|term| match term {
            Term::NamedNode(node) => Some(node.as_ref()),
            _ => None,
        });
        let ordinary = (default_graph
            && (subject.is_none() || standard_subject.is_some())
            && (predicate.is_none() || standard_predicate.is_some()))
        .then(|| {
            data.quads(
                standard_subject,
                standard_predicate,
                object.map(Term::as_ref),
            )
        })
        .into_iter()
        .flatten()
        .map(|quad| {
            let quad = quad?;
            Ok(InternalQuad {
                subject: quad.subject.into(),
                predicate: quad.predicate.into(),
                object: quad.object,
                graph_name: None,
            })
        });
        let generalized = extra
            .matching(pattern)
            .filter(move |_| default_graph || generalized_graph)
            .map(move |row| {
                Ok(InternalQuad {
                    subject: row[0].into_owned(),
                    predicate: row[1].into_owned(),
                    object: row[2].into_owned(),
                    graph_name: generalized_graph.then(|| GENERALIZED_GRAPH.into_owned().into()),
                })
            });
        Box::new(ordinary.chain(generalized))
    }

    fn internalize_term(&self, term: Term) -> Result<Term> {
        Ok(term)
    }

    fn externalize_term(&self, term: Term) -> Result<Term> {
        Ok(term)
    }
}
