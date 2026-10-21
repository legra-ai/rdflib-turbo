//! Typed query parameters never become interpolated SPARQL source text.

use std::collections::BTreeMap;

use oxigraph::model::{Term, Variable};
use oxigraph::sparql::PreparedSparqlQuery;

use super::operations::invalid;
use super::pattern::InputTerm;
use crate::Result;

pub(super) struct InitialBindings {
    // bounded: parameters supplied for this one query, never stored RDF rows.
    values: Option<BTreeMap<String, Option<InputTerm>>>,
}

impl InitialBindings {
    pub(super) fn parse(source: Option<&str>) -> Result<Self> {
        Ok(Self {
            values: source
                .map(serde_json::from_str)
                .transpose()
                .map_err(invalid)?,
        })
    }

    pub(super) fn names(&self) -> impl Iterator<Item = &str> {
        self.values
            .iter()
            .flat_map(|values| values.keys().map(String::as_str))
    }

    fn decode(self) -> Result<Vec<(Variable, Term)>> {
        self.values
            .into_iter()
            .flatten()
            .filter_map(|(variable, term)| {
                term.map(|term| {
                    term.term()
                        .map(|term| (Variable::new_unchecked(variable), term))
                })
            })
            .collect()
    }

    pub(super) fn apply(self, query: PreparedSparqlQuery) -> Result<PreparedSparqlQuery> {
        if self.values.is_none() {
            return Ok(query);
        }
        Ok(query.with_initial_bindings(self.decode()?))
    }
}

pub(super) fn apply(
    query: PreparedSparqlQuery,
    source: Option<&str>,
) -> Result<PreparedSparqlQuery> {
    InitialBindings::parse(source)?.apply(query)
}

pub(super) fn decode(source: &str) -> Result<Vec<(Variable, Term)>> {
    InitialBindings::parse(Some(source))?.decode()
}
