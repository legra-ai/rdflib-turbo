//! Public inference entry points over datasets and active transactions.

use oxigraph::model::*;
use oxrdf::GeneralizedDataset;

use super::options::expand_configured;
use super::{InferenceOutcome, InferenceRequest, backend, staged, standard_numeric};
use crate::Result;

pub(crate) fn supports_mode(mode: &str) -> bool {
    matches!(mode, "rdfs" | "owlrl" | "both" | standard_numeric::MODE)
}

pub(crate) fn expand(
    data: &mut GeneralizedDataset,
    mode: &str,
    request: InferenceRequest,
) -> Result<InferenceOutcome> {
    if data.iter().any(|q| !q.graph_name.is_default_graph()) {
        return expand_into(data, mode, GraphName::DefaultGraph, request);
    }
    let mut transaction = data.transaction();
    let (_, outcome) = expand_configured(&mut transaction, mode, request)?;
    transaction.commit();
    Ok(outcome)
}

/// Infer over the dataset union and put output in the exact destination.
/// Original graph contexts remain available to callers and report descriptions.
pub(crate) fn expand_into(
    data: &mut GeneralizedDataset,
    mode: &str,
    destination: GraphName,
    request: InferenceRequest,
) -> Result<InferenceOutcome> {
    staged::expand(data, mode, destination, request)
}

/// Expand directly over the active transaction's union, without copying its base.
pub(crate) fn expand_transaction(
    transaction: &mut oxigraph::store::Transaction<'_>,
    mode: &str,
    destination: GraphName,
    request: InferenceRequest,
) -> Result<InferenceOutcome> {
    expand_configured(
        &mut backend::TransactionGraph {
            transaction,
            destination,
        },
        mode,
        request,
    )
    .map(|(_, outcome)| outcome)
}
