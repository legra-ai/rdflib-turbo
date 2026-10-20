//! Export additions from the insertion journal before committing inference.

use oxigraph::model::{Dataset, GraphName, Quad};

use super::expand_inner;
use crate::Result;

pub(crate) fn expand_delta(
    data: &mut Dataset,
    mode: &str,
    destination: Option<GraphName>,
) -> Result<Dataset> {
    if destination.is_none() && data.iter().all(|quad| quad.graph_name.is_default_graph()) {
        let mut transaction = data.insert_transaction();
        expand_inner(&mut transaction, mode)?;
        // The returned dataset owns only derived output, never a base snapshot.
        let delta = transaction.inserted().collect();
        transaction.commit();
        return Ok(delta);
    }

    // Retain Dataset inference's existing union semantics for named contexts.
    let mut union = crate::graph::with_union(data);
    let mut union_transaction = union.insert_transaction();
    expand_inner(&mut union_transaction, mode)?;
    let destination = destination.unwrap_or(GraphName::DefaultGraph);
    let mut transaction = data.insert_transaction();
    for quad in union_transaction.inserted() {
        transaction.insert(&Quad::new(
            quad.subject,
            quad.predicate,
            quad.object,
            destination.clone(),
        ));
    }
    // The base journal excludes any exact quad already present at destination.
    let delta = transaction.inserted().collect();
    union_transaction.commit();
    transaction.commit();
    Ok(delta)
}
