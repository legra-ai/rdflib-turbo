//! Export additions from the insertion journal before committing inference.

use oxigraph::model::{Dataset, GraphName, Quad};

use super::{InferenceOutcome, InferenceRequest, expand_configured};
use crate::Result;

pub(crate) fn expand_delta(
    data: &mut Dataset,
    mode: &str,
    destination: Option<GraphName>,
    request: InferenceRequest,
) -> Result<(Dataset, InferenceOutcome)> {
    if destination.is_none() && data.iter().all(|quad| quad.graph_name.is_default_graph()) {
        let mut transaction = data.insert_transaction();
        let (_, outcome) = expand_configured(&mut transaction, mode, request)?;
        // The returned dataset owns only derived output, never a base snapshot.
        let delta = transaction.inserted().collect();
        transaction.commit();
        return Ok((delta, outcome));
    }

    let destination = destination.unwrap_or(GraphName::DefaultGraph);
    let (additions, outcome) = super::staged::expand(data, mode, destination.clone(), request)?;
    let mut transaction = data.insert_transaction();
    for quad in &additions {
        transaction.insert(&Quad::new(
            quad.subject,
            quad.predicate,
            quad.object,
            destination.clone(),
        ));
    }
    // The base journal excludes any exact quad already present at destination.
    let delta = transaction.inserted().collect();
    transaction.commit();
    Ok((delta, outcome))
}
