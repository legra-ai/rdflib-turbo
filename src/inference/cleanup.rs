//! Original OWL postprocess removes exact source-enumerated blank-predicate SPOs.
use oxigraph::model::*;
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Transaction;

use crate::{Error, Result};

pub(super) fn transaction(
    transaction: &mut Transaction<'_>,
    source: Option<&GraphName>,
    destination: &GraphName,
) -> Result<()> {
    let pattern = if source.is_some() {
        "?s ?p ?o FILTER(isBlank(?p))"
    } else {
        "{ ?s ?p ?o FILTER(isBlank(?p)) } UNION { GRAPH ?g { ?s ?p ?o FILTER(isBlank(?p)) } }"
    };
    let mut query = SparqlEvaluator::new()
        .parse_query(&format!("SELECT ?s ?p ?o ?g WHERE {{ {pattern} }}"))
        .map_err(|error| Error::Runtime(error.to_string()))?;
    if let Some(source) = source {
        // bounded: one explicitly selected physical source graph.
        query.dataset_mut().set_default_graph(vec![source.clone()]);
    }
    // Retain only actual changed rows, never the base relation. Drop the reader
    // before mutation: persistent transaction iterators borrow their writer.
    let mut removals = None;
    {
        let QueryResults::Solutions(rows) = query.on_transaction(transaction).execute()? else {
            unreachable!("cleanup SELECT must return solutions");
        };
        for row in rows {
            let row = row?;
            let field = |name| {
                row.get(name)
                    .cloned()
                    .ok_or_else(|| Error::Runtime(format!("Unbound cleanup {name}")))
            };
            let graph = match source {
                Some(source) => source.clone(),
                None => match row.get("g") {
                    None => GraphName::DefaultGraph,
                    Some(Term::NamedNode(node)) => node.clone().into(),
                    Some(Term::BlankNode(node)) => node.clone().into(),
                    Some(_) => return Err(Error::Runtime("Invalid cleanup graph kind".into())),
                },
            };
            removals
                .get_or_insert_with(GeneralizedDataset::new)
                .insert(&GeneralizedQuad::new(
                    field("s")?,
                    field("p")?,
                    field("o")?,
                    graph,
                ));
        }
    }
    if let Some(removals) = removals {
        for quad in removals.iter() {
            transaction.remove_generalized(quad);
            transaction.remove_generalized(GeneralizedQuadRef {
                graph_name: destination.as_ref(),
                ..quad
            });
        }
    }
    Ok(())
}
