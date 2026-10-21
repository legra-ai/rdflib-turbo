//! Typed query parameters never become interpolated SPARQL source text.

use std::collections::BTreeMap;

use oxigraph::model::Variable;
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
    let bindings: BTreeMap<String, Option<InputTerm>> =
        serde_json::from_str(source).map_err(invalid)?;
    let values = bindings
        .into_iter()
        .filter_map(|(variable, term)| {
            term.map(|term| {
                term.term()
                    .map(|term| (Variable::new_unchecked(variable), term))
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(query.with_initial_bindings(values))
}
