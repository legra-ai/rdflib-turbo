//! Delay only reflexive rows and retire preparation flags before composition.
use std::collections::HashMap;

use oxigraph::model::*;

use super::{Graph, Pending, Rows, row};
use crate::Result;

impl Rows {
    pub(super) fn symmetry(
        &mut self,
        data: &impl Graph,
        predicate: &NamedNode,
        pending: &mut Pending,
        signatures: &HashMap<(usize, u64), Vec<usize>>,
    ) -> Result<()> {
        for (&(degree, _), groups) in signatures {
            for &id in groups {
                let members = &self.groups[id].members;
                let deferred = |member: &Term| self.subjects[member].symmetry_is_deferred();
                if !members.iter().any(deferred) {
                    continue;
                }
                // Every member belongs to the common row. Equal cardinalities
                // therefore prove this row is exactly the closed member set.
                if degree == members.len() && members.iter().all(deferred) {
                    continue;
                }
                // Retain only this one common outgoing row, never all row sets.
                for target in row(data, &members[0], predicate)? {
                    if self
                        .subjects
                        .get(&target)
                        .is_some_and(|subject| subject.group() == id)
                    {
                        // Deferred members are already in this identical row.
                        continue;
                    }
                    for member in members.iter().filter(|member| deferred(member)) {
                        pending.push(
                            data,
                            GeneralizedQuad::new(
                                target.clone(),
                                predicate.clone(),
                                member.clone(),
                                GraphName::DefaultGraph,
                            ),
                        )?;
                    }
                }
            }
        }
        // Construction owns this temporary bit. Composition receives clean
        // slots and reuses it exclusively for cross-group incoming witnesses.
        for subject in self.subjects.values_mut() {
            subject.finish_preparation();
        }
        Ok(())
    }
}
