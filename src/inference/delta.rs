//! Export additions from the insertion journal before committing inference.

use oxigraph::model::GraphName;
use oxrdf::GeneralizedDataset;

use super::{InferenceOutcome, InferenceRequest, expand_configured};
use crate::Result;

pub(crate) fn expand_delta(
    data: &mut GeneralizedDataset,
    mode: &str,
    destination: Option<GraphName>,
    request: InferenceRequest,
) -> Result<(GeneralizedDataset, InferenceOutcome)> {
    if destination.is_none() && data.iter().all(|quad| quad.graph_name.is_default_graph()) {
        let mut transaction = data.transaction();
        let (_, outcome) = expand_configured(&mut transaction, mode, request)?;
        // The returned dataset owns only derived output, never a base snapshot.
        let delta = transaction.inserted().collect();
        transaction.commit();
        return Ok((delta, outcome));
    }

    let destination = destination.unwrap_or(GraphName::DefaultGraph);
    let mut transaction = super::staged::JournalGraph::new(data, destination);
    let (_, outcome) = expand_configured(&mut transaction, mode, request)?;
    // The base journal excludes any exact quad already present at destination.
    let delta = transaction.inserted().collect();
    transaction.commit();
    Ok((delta, outcome))
}
