//! Export additions from the insertion journal before committing inference.

use oxigraph::model::{Dataset, GraphName, Quad};

use super::{InferenceOptions, expand_configured};
use crate::Result;

/// Stage only direct literal publications absent from the exact output graph.
/// The returned dataset is an output delta, never a copy of source contexts.
pub(super) fn literal_publications(data: &Dataset, mode: &str, destination: &GraphName) -> Dataset {
    if mode != "rdfs" {
        return Dataset::new();
    }
    data.iter()
        .filter(|quad| quad.object.is_literal())
        .map(|quad| {
            Quad::new(
                quad.subject,
                quad.predicate,
                quad.object,
                destination.clone(),
            )
        })
        .filter(|quad| !data.contains(quad))
        .collect()
}

pub(crate) fn expand_delta(
    data: &mut Dataset,
    mode: &str,
    destination: Option<GraphName>,
    options: InferenceOptions,
) -> Result<Dataset> {
    if destination.is_none() && data.iter().all(|quad| quad.graph_name.is_default_graph()) {
        let mut transaction = data.insert_transaction();
        expand_configured(&mut transaction, mode, options)?;
        // The returned dataset owns only derived output, never a base snapshot.
        let delta = transaction.inserted().collect();
        transaction.commit();
        return Ok(delta);
    }

    // Retain Dataset inference's existing union semantics for named contexts.
    let mut union = crate::graph::with_union(data);
    let mut union_transaction = union.insert_transaction();
    let prepared = expand_configured(&mut union_transaction, mode, options)?;
    let destination = destination.unwrap_or(GraphName::DefaultGraph);
    let publications = literal_publications(data, mode, &destination);
    let mut transaction = data.insert_transaction();
    for quad in publications.iter() {
        transaction.insert(quad);
    }
    for quad in union_transaction.inserted() {
        transaction.insert(&Quad::new(
            quad.subject,
            quad.predicate,
            quad.object,
            destination.clone(),
        ));
    }
    prepared.insert(|mut quad| {
        quad.graph_name = destination.clone();
        transaction.insert(&quad);
    })?;
    // The base journal excludes any exact quad already present at destination.
    let delta = transaction.inserted().collect();
    union_transaction.commit();
    transaction.commit();
    Ok(delta)
}
