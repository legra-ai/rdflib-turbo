use crate::graph::*;
use crate::{Error, Result};
use oxigraph::model::*;
use std::collections::HashSet;

pub(crate) fn rdf_list(store: &Dataset, head: &Term) -> Result<Vec<Term>> {
    let mut node = head.clone();
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    while node != Term::from(iri(RDF, "nil")) {
        subject(&node)?;
        if !seen.insert(node.clone()) {
            return Err(Error::Invalid(format!("Cyclic RDF list at {node}")));
        }
        let first = objects(store, &node, &iri(RDF, "first"))?;
        let rest = objects(store, &node, &iri(RDF, "rest"))?;
        if first.len() != 1 || rest.len() != 1 {
            return Err(Error::Invalid(format!("Malformed RDF list at {node}")));
        }
        result.push(first[0].clone());
        node = rest[0].clone();
    }
    Ok(result)
}
