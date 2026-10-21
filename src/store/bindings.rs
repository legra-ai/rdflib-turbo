//! Typed query parameters never become interpolated SPARQL source text.

use std::collections::BTreeMap;

use oxigraph::model::{Term, Variable};
use oxigraph::sparql::PreparedSparqlQuery;

use super::operations::invalid;
use super::pattern::InputTerm;
use crate::Result;

pub(super) fn apply(
    query: PreparedSparqlQuery,
    source: Option<&str>,
) -> Result<PreparedSparqlQuery> {
    let Some(source) = source else {
        return Ok(query);
    };
    Ok(query.with_initial_bindings(decode(source)?))
}

pub(super) fn decode(source: &str) -> Result<Vec<(Variable, Term)>> {
    let bindings: BTreeMap<String, Option<InputTerm>> =
        serde_json::from_str(source).map_err(invalid)?;
    bindings
        .into_iter()
        .filter_map(|(variable, term)| {
            term.map(|term| {
                term.term()
                    .map(|term| (Variable::new_unchecked(variable), term))
            })
        })
        .collect()
}
