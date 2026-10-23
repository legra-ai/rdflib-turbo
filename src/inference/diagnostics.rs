//! Standard-RDF inconsistency messages, published only after successful closure.

use std::collections::HashSet;
use std::sync::Arc;

use oxigraph::model::*;

use super::backend::Graph;
use super::diagnostic_datatypes::{self, LexicalContext, LexicalResult};
use super::outcome::{ERROR_NAMESPACE, InferenceOutcome};
use crate::graph::{RDF, iri};
use crate::{Error, Result};

mod lists;
mod rdf_list;
mod relations;
mod restrictions;

#[derive(Default)]
pub(super) struct Diagnostics {
    // Output-sized: the caller's prior messages plus newly discovered diagnostics.
    messages: Vec<Arc<String>>,
    known: HashSet<Arc<String>>,
    list_witnesses: rdf_list::ListWitnesses,
}

impl Diagnostics {
    pub(super) fn new(prior: Vec<String>) -> Self {
        let mut diagnostics = Self::default();
        for message in prior {
            let message = Arc::new(message);
            diagnostics.known.insert(message.clone());
            diagnostics.messages.push(message);
        }
        diagnostics
    }

    fn add(&mut self, message: String) {
        let message = Arc::new(message);
        if self.known.insert(message.clone()) {
            self.messages.push(message);
        }
    }

    pub(super) fn literal(
        &mut self,
        literal: &Literal,
        valid: impl Fn(&Literal) -> Result<bool>,
    ) -> Result<()> {
        if !valid(literal)? {
            self.add(format!(
                "Lexical value of the literal '{}' does not match its datatype ({})",
                literal.value(),
                literal.datatype().as_str()
            ));
        }
        Ok(())
    }

    pub(super) fn round(&mut self, data: &impl Graph) -> Result<()> {
        let witnesses = std::mem::take(&mut self.list_witnesses);
        let result = lists::collect(data, self, &witnesses);
        self.list_witnesses = witnesses;
        result
    }

    pub(super) fn finish(
        mut self,
        data: &mut impl Graph,
        owl: bool,
        is_zero: impl Fn(&Term) -> Result<bool>,
    ) -> Result<InferenceOutcome> {
        if owl {
            relations::collect(data, &mut self)?;
            restrictions::collect(data, &mut self, is_zero)?;
        }
        drop(self.known);
        drop(self.list_witnesses);
        let error_messages: Vec<String> = self
            .messages
            .into_iter()
            .map(Arc::unwrap_or_clone)
            .collect();
        for message in &error_messages {
            let subject = BlankNode::default();
            data.insert(Quad::new(
                subject.clone(),
                iri(RDF, "type"),
                iri(ERROR_NAMESPACE, "ErrorMessage"),
                GraphName::DefaultGraph,
            ))?;
            data.insert(Quad::new(
                subject,
                iri(ERROR_NAMESPACE, "error"),
                Literal::new_simple_literal(message.as_str()),
                GraphName::DefaultGraph,
            ))?;
        }
        Ok(InferenceOutcome { error_messages })
    }
}

fn subject(term: &Term) -> Option<NamedOrBlankNodeRef<'_>> {
    match term {
        Term::NamedNode(node) => Some(node.as_ref().into()),
        Term::BlankNode(node) => Some(node.as_ref().into()),
        Term::Literal(_) => None,
    }
}

fn text(term: &Term) -> &str {
    match term {
        Term::NamedNode(node) => node.as_str(),
        Term::BlankNode(node) => node.as_str(),
        Term::Literal(literal) => literal.value(),
    }
}

fn has(data: &impl Graph, left: &Term, predicate: &NamedNode, right: &Term) -> Result<bool> {
    let Some(left) = subject(left) else {
        return Ok(false);
    };
    data.quads(Some(left), Some(predicate.as_ref()), Some(right.as_ref()))
        .next()
        .transpose()
        .map(|quad| quad.is_some())
}

fn objects<'a>(
    data: &'a impl Graph,
    node: &'a Term,
    predicate: &'a NamedNode,
) -> impl Iterator<Item = Result<Term>> + 'a {
    subject(node)
        .into_iter()
        .flat_map(move |node| data.quads(Some(node), Some(predicate.as_ref()), None))
        .map(|quad| quad.map(|quad| quad.object))
}

pub(super) fn valid_lexical(literal: &Literal, context: &LexicalContext) -> Result<bool> {
    match diagnostic_datatypes::check(literal, context) {
        LexicalResult::Valid => Ok(true),
        LexicalResult::Diagnostic => Ok(false),
        LexicalResult::Failure(failure) => Err(Error::Lexical(failure)),
    }
}

pub(super) fn cardinality_zero(term: &Term, context: &LexicalContext) -> Result<bool> {
    match term {
        Term::Literal(literal) => {
            diagnostic_datatypes::cardinality_zero(literal, context).map_err(Error::Lexical)
        }
        Term::NamedNode(_) => Err(Error::Attribute(
            "'URIRef' object has no attribute 'value'".into(),
        )),
        Term::BlankNode(_) => Err(Error::Attribute(
            "'BNode' object has no attribute 'value'".into(),
        )),
    }
}
