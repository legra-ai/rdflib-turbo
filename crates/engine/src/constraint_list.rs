//! Membership collections follow RDFLib.items, independently from path syntax.
use std::collections::HashSet;

use oxigraph::model::*;

use crate::graph::{RDF, iri};
use crate::{Error, GraphRead, Result};

#[path = "constraint_list_truthiness.rs"]
mod truthiness;

#[cfg(test)]
#[path = "constraint_list_tests.rs"]
mod tests;

pub(super) fn read(
    source: &(impl GraphRead + ?Sized),
    head: &Term,
    digits: crate::constraints::DecimalDigitPolicy,
) -> Result<Vec<Term>> {
    let mut cursor = Some(head.clone());
    let mut seen = HashSet::from([head.clone()]);
    let mut values = Vec::new();
    let first = iri(RDF, "first");
    let rest = iri(RDF, "rest");
    while let Some(node) = cursor {
        if !truthiness::truthy_with_policy(&node, digits) {
            break;
        }
        if let Some(value) = one_value(source, &node, &first)? {
            values.push(value);
        }
        cursor = one_value(source, &node, &rest)?;
        if let Some(next) = &cursor
            && !seen.insert(next.clone())
        {
            return Err(Error::Invalid(
                "List contains a recursive rdf:rest reference".into(),
            ));
        }
    }
    Ok(values)
}

fn one_value(
    source: &(impl GraphRead + ?Sized),
    node: &Term,
    predicate: &NamedNode,
) -> Result<Option<Term>> {
    // Graph.value(any=True) consumes one indexed witness, not all matching rows.
    source
        .matching_quads(
            Some(node.as_ref()),
            Some(predicate.as_ref().into()),
            None,
            Some(GraphNameRef::DefaultGraph),
        )
        .next()
        .transpose()
        .map(|row| row.map(|quad| quad.object))
}
