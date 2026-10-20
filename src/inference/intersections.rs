//! Indexed intersection candidates, preserving the original property-path semantics.
use std::collections::HashSet;

use oxigraph::model::*;

use super::backend::Graph;
use super::{OWL, Pending};
use crate::Result;
use crate::graph::{RDF, iri};

// This is rdf:rest*/rdf:first reachability, not strict RDF-list validation.
// Empty, dangling, branching and cyclic paths retain their query semantics.
fn members(data: &impl Graph, head: &Term) -> Result<HashSet<Term>> {
    let first = iri(RDF, "first");
    let rest = iri(RDF, "rest");
    let mut seen = HashSet::from([head.clone()]);
    let mut pending = vec![head.clone()];
    let mut members = HashSet::new();
    while let Some(node) = pending.pop() {
        let subject = match &node {
            Term::NamedNode(node) => node.as_ref().into(),
            Term::BlankNode(node) => node.as_ref().into(),
            Term::Literal(_) => continue,
        };
        for quad in data.quads(Some(subject), Some(first.as_ref()), None) {
            members.insert(quad?.object);
        }
        for quad in data.quads(Some(subject), Some(rest.as_ref()), None) {
            let next = quad?.object;
            if seen.insert(next.clone()) {
                pending.push(next);
            }
        }
    }
    Ok(members)
}

pub(super) fn additions(data: &impl Graph, additions: &mut Pending) -> Result<()> {
    let rdf_type = iri(RDF, "type");
    for intersection in data.quads(None, Some(iri(OWL, "intersectionOf").as_ref()), None) {
        let intersection = intersection?;
        let members = members(data, &intersection.object)?;
        // Any required member is a valid indexed seed. An empty reachable set
        // retains the original universal match over subjects with any type.
        let seed = members.iter().next().map(Term::as_ref);
        for candidate in data.quads(None, Some(rdf_type.as_ref()), seed) {
            let candidate = candidate?;
            let mut matches = true;
            for member in &members {
                if !data.contains(&Quad::new(
                    candidate.subject.clone(),
                    rdf_type.clone(),
                    member.clone(),
                    GraphName::DefaultGraph,
                ))? {
                    matches = false;
                    break;
                }
            }
            if matches {
                additions.push(
                    data,
                    Quad::new(
                        candidate.subject,
                        rdf_type.clone(),
                        intersection.subject.clone(),
                        GraphName::DefaultGraph,
                    ),
                )?;
            }
        }
    }
    Ok(())
}
