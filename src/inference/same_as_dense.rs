//! Probe missing pairs for dense output rows instead of enumerating existing witnesses.
use std::collections::HashSet;

use oxigraph::model::*;

use super::{Graph, Pending, Rows};
use crate::Result;

fn edge(data: &impl Graph, s: &NamedOrBlankNode, p: &NamedNode, o: &Term) -> Result<bool> {
    data.quads(Some(s.as_ref()), Some(p.as_ref()), Some(o.as_ref()))
        .next()
        .transpose()
        .map(|quad| quad.is_some())
}

fn successor_witness(
    data: &impl Graph,
    predicate: &NamedNode,
    values: &HashSet<Term>,
    target: &Term,
) -> Result<bool> {
    for intermediate in values {
        let Ok(intermediate) = NamedOrBlankNode::try_from(intermediate.clone()) else {
            continue;
        };
        if edge(data, &intermediate, predicate, target)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn predecessor_witness(
    data: &impl Graph,
    predicate: &NamedNode,
    subject: &NamedOrBlankNode,
    target: &Term,
) -> Result<bool> {
    for predecessor in data.quads(
        None,
        Some(predicate.as_ref()),
        Some(subject.as_ref().into()),
    ) {
        if edge(data, &predecessor?.subject, predicate, target)? {
            return Ok(true);
        }
    }
    Ok(false)
}

impl Rows {
    pub(super) fn dense(&self, subject: &NamedOrBlankNode) -> bool {
        self.subjects
            .get(subject)
            .is_some_and(|&id| self.components.is_dense(id))
    }

    pub(super) fn extend_dense(
        &self,
        data: &impl Graph,
        predicate: &NamedNode,
        values: &HashSet<Term>,
        members: &[NamedOrBlankNode],
        pending: &mut Pending,
    ) -> Result<()> {
        for target in self.components.members(self.subjects[&members[0]]) {
            if values.contains(target) {
                continue;
            }
            // RR is shared by identical outgoing rows. R^-1R depends on each
            // member's incoming row and must be checked independently.
            let transitive = successor_witness(data, predicate, values, target)?;
            for member in members {
                if transitive || predecessor_witness(data, predicate, member, target)? {
                    pending.push(
                        data,
                        Quad::new(
                            member.clone(),
                            predicate.clone(),
                            target.clone(),
                            GraphName::DefaultGraph,
                        ),
                    )?;
                }
            }
        }
        Ok(())
    }
}
