//! The diagnostic message collector and its closure-time checks.

use std::collections::HashSet;
use std::sync::Arc;

use oxigraph::model::*;

use super::super::backend::Graph;
use super::super::outcome::{ERROR_NAMESPACE, InferenceOutcome};
use super::{lists, rdf_list, relations, restrictions};
use crate::Result;
use crate::graph::{RDF, iri};

pub(in crate::inference) struct Diagnostics {
    // Output-sized: the caller's prior messages plus newly discovered diagnostics.
    messages: Vec<Arc<String>>,
    known: HashSet<Arc<String>>,
    list_witnesses: rdf_list::ListWitnesses,
}

impl Diagnostics {
    pub(in crate::inference) fn new(prior: Vec<String>, integer_limit: usize) -> Self {
        let mut diagnostics = Self {
            messages: Vec::new(),
            known: HashSet::new(),
            list_witnesses: rdf_list::ListWitnesses::new(integer_limit),
        };
        for message in prior {
            let message = Arc::new(message);
            diagnostics.known.insert(message.clone());
            diagnostics.messages.push(message);
        }
        diagnostics
    }

    pub(in crate::inference) fn add(&mut self, message: String) {
        let message = Arc::new(message);
        if self.known.insert(message.clone()) {
            self.messages.push(message);
        }
    }

    pub(in crate::inference) fn literal(
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

    pub(in crate::inference) fn round(&mut self, data: &impl Graph) -> Result<()> {
        let limit = self.list_witnesses.integer_limit();
        let witnesses = std::mem::replace(
            &mut self.list_witnesses,
            rdf_list::ListWitnesses::new(limit),
        );
        let result = lists::collect(data, self, &witnesses);
        self.list_witnesses = witnesses;
        result
    }

    pub(in crate::inference) fn witnesses(&self) -> &rdf_list::ListWitnesses {
        &self.list_witnesses
    }

    pub(in crate::inference) fn finish(
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
            data.insert(GeneralizedQuad::new(
                subject.clone(),
                iri(RDF, "type"),
                iri(ERROR_NAMESPACE, "ErrorMessage"),
                GraphName::DefaultGraph,
            ))?;
            data.insert(GeneralizedQuad::new(
                subject,
                iri(ERROR_NAMESPACE, "error"),
                Literal::new_simple_literal(message.as_str()),
                GraphName::DefaultGraph,
            ))?;
        }
        Ok(InferenceOutcome { error_messages })
    }
}
