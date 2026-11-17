//! Equality replacements published for each sameAs-like predicate.
use oxigraph::model::*;

use super::super::{Graph, OWL, Pending};
use super::rows::Rows;
use crate::Result;

fn replacements(data: &impl Graph, predicate: &NamedNode, pending: &mut Pending) -> Result<()> {
    for quad in data.quads(None, None, None) {
        let quad = quad?;
        // Equality-predicate consequences are R R / R^-1 R above. Reject these
        // before alias lookups, rather than filtering a cubic SPARQL witness bag.
        if quad.predicate.as_ref() == TermRef::NamedNode(predicate.as_ref()) {
            continue;
        }
        for alias in data.quads(
            Some(quad.subject.as_ref()),
            Some(predicate.as_ref().into()),
            None,
        ) {
            pending.push(
                data,
                GeneralizedQuad::new(
                    alias?.object,
                    quad.predicate.clone(),
                    quad.object.clone(),
                    GraphName::DefaultGraph,
                ),
            )?;
        }
        for alias in data.quads(
            Some(quad.object.as_ref()),
            Some(predicate.as_ref().into()),
            None,
        ) {
            pending.push(
                data,
                GeneralizedQuad::new(
                    quad.subject.clone(),
                    quad.predicate.clone(),
                    alias?.object,
                    GraphName::DefaultGraph,
                ),
            )?;
        }
    }
    Ok(())
}

pub(in crate::inference) fn apply(data: &impl Graph, pending: &mut Pending) -> Result<()> {
    let predicate = NamedNode::new_unchecked(format!("{OWL}sameAs"));
    let mut rows = Rows::new(data, &predicate, pending)?;
    if rows.groups.is_empty() {
        return Ok(());
    }
    rows.extend(data, &predicate, pending)?;
    replacements(data, &predicate, pending)
}
