//! Indexed intersection candidates, preserving the original property-path semantics.
use std::collections::HashSet;

use oxigraph::model::*;

use super::backend::Graph;
use super::generalized::{Generalized, Tuples};
use super::{OWL, Pending};
use crate::Result;
use crate::graph::{RDF, iri};

// This is rdf:rest*/rdf:first reachability, not strict RDF-list validation.
// Empty, dangling, branching and cyclic paths retain their query semantics.
fn members(data: &Tuples<'_, impl Graph>, head: &Term) -> Result<HashSet<Term>> {
    let first = iri(RDF, "first");
    let rest = iri(RDF, "rest");
    let mut seen = HashSet::from([head.clone()]);
    let mut pending = vec![head.clone()];
    let mut members = HashSet::new();
    while let Some(node) = pending.pop() {
        for row in data.matching([Some(node.as_ref()), Some(first.as_ref().into()), None]) {
            members.insert(row?[2].clone());
        }
        for row in data.matching([Some(node.as_ref()), Some(rest.as_ref().into()), None]) {
            let next = row?[2].clone();
            if seen.insert(next.clone()) {
                pending.push(next);
            }
        }
    }
    Ok(members)
}

pub(super) fn additions(
    data: &Tuples<'_, impl Graph>,
    additions: &mut Pending,
    generalized: &mut Generalized,
) -> Result<()> {
    let rdf_type: Term = iri(RDF, "type").into();
    for intersection in
        data.matching([None, Some(iri(OWL, "intersectionOf").as_ref().into()), None])
    {
        let intersection = intersection?;
        let members = members(data, &intersection[2])?;
        // Any required member is a valid indexed seed. An empty reachable set
        // retains the original universal match over subjects with any type.
        let seed = members.iter().next().map(Term::as_ref);
        for candidate in data.matching([None, Some(rdf_type.as_ref()), seed]) {
            let candidate = candidate?;
            let mut matches = true;
            for member in &members {
                if !data.contains(&[candidate[0].clone(), rdf_type.clone(), member.clone()])? {
                    matches = false;
                    break;
                }
            }
            if matches {
                data.push(
                    [
                        candidate[0].clone(),
                        rdf_type.clone(),
                        intersection[0].clone(),
                    ],
                    additions,
                    generalized,
                )?;
            }
        }
    }
    Ok(())
}
