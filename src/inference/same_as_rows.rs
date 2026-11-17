//! Subject rows of one equality predicate, grouped by their value sets.
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};

use oxigraph::model::*;

use super::super::{Graph, Pending};
use super::components::Components;
use crate::Result;

pub(super) struct RowGroup {
    pub(super) members: Vec<Term>,
}

#[derive(Clone, Copy)]
#[repr(transparent)]
pub(super) struct SubjectRow(usize);

impl SubjectRow {
    pub(super) fn new(group: usize) -> Self {
        // A physical Vec<RowGroup> already bounds indices below
        // isize::MAX / size_of::<RowGroup>(), leaving room for one flag bit.
        Self(
            group
                .checked_mul(2)
                .expect("equality row index exceeds its subject slot"),
        )
    }

    pub(super) fn group(self) -> usize {
        self.0 >> 1
    }

    fn prepared(group: usize, deferred: bool) -> Self {
        Self(Self::new(group).0 | usize::from(deferred))
    }

    pub(super) fn symmetry_is_deferred(self) -> bool {
        self.0 & 1 != 0
    }

    pub(super) fn finish_preparation(&mut self) {
        self.0 &= !1;
    }

    pub(super) fn has_cross_group_incoming(self) -> bool {
        self.0 & 1 != 0
    }

    pub(super) fn mark_cross_group_incoming(&mut self) {
        self.0 |= 1;
    }
}

pub(super) struct Rows {
    pub(super) subjects: HashMap<Term, SubjectRow>,
    pub(super) groups: Vec<RowGroup>,
    pub(super) components: Components,
}

pub(super) fn row(
    data: &impl Graph,
    subject: &Term,
    predicate: &NamedNode,
) -> Result<HashSet<Term>> {
    data.quads(
        Some(subject.as_ref()),
        Some(predicate.as_ref().into()),
        None,
    )
    .map(|quad| quad.map(|quad| quad.object))
    .collect()
}

fn signature(values: &HashSet<Term>) -> (usize, u64) {
    let mut signature = 0;
    for value in values {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        signature ^= hasher.finish();
    }
    (values.len(), signature)
}

impl Rows {
    pub(super) fn new(
        data: &impl Graph,
        predicate: &NamedNode,
        pending: &mut Pending,
    ) -> Result<Self> {
        // Transient O(subjects) identities/groups, never a retained copy of all equality edges.
        let mut output = Self {
            subjects: HashMap::new(),
            groups: Vec::new(),
            components: Components::default(),
        };
        let mut signatures: HashMap<(usize, u64), Vec<usize>> = HashMap::new();
        for quad in data.quads(None, Some(predicate.as_ref().into()), None) {
            let quad = quad?;
            if output.subjects.contains_key(&quad.subject) {
                continue;
            }
            // Only this outgoing row is materialized; discard it before the next subject.
            let values = row(data, &quad.subject, predicate)?;
            let signature = signature(&values);
            let mut matched = None;
            for &group in signatures.get(&signature).into_iter().flatten() {
                let representative = &output.groups[group].members[0];
                let mut equal = true;
                for candidate in data.quads(
                    Some(representative.as_ref()),
                    Some(predicate.as_ref().into()),
                    None,
                ) {
                    if !values.contains(&candidate?.object) {
                        equal = false;
                        break;
                    }
                }
                // Signature lengths are exact distinct-row sizes, even for union duplicates.
                // All representative members in an equally sized set proves equality.
                if equal {
                    matched = Some(group);
                    break;
                }
            }
            let group = matched.unwrap_or_else(|| {
                let index = output.groups.len();
                output.groups.push(RowGroup {
                    members: Vec::new(),
                });
                signatures.entry(signature).or_default().push(index);
                index
            });
            output.groups[group].members.push(quad.subject.clone());
            let deferred = values.contains(&quad.subject);
            if !deferred {
                for target in values {
                    pending.push(
                        data,
                        GeneralizedQuad::new(
                            target,
                            predicate.clone(),
                            quad.subject.clone(),
                            GraphName::DefaultGraph,
                        ),
                    )?;
                }
            }
            output
                .subjects
                .insert(quad.subject, SubjectRow::prepared(group, deferred));
        }
        output.symmetry(data, predicate, pending, &signatures)?;
        output.components = Components::discover(data, predicate, &output, &signatures)?;
        Ok(output)
    }

    pub(super) fn extend(
        &mut self,
        data: &impl Graph,
        predicate: &NamedNode,
        pending: &mut Pending,
    ) -> Result<()> {
        for (id, group) in self.groups.iter().enumerate() {
            let values = row(data, &group.members[0], predicate)?;
            let dense = self.dense(&group.members[0]);
            if dense {
                self.extend_dense(data, predicate, &values, &group.members, pending)?;
            }
            // Transient missing-target delta for this group only, not a second edge relation.
            let mut missing = HashSet::new();
            for intermediate in &values {
                if self
                    .subjects
                    .get(intermediate)
                    .is_some_and(|row| row.group() == id)
                {
                    continue;
                }
                // This representative row proves an incoming witness from this
                // source group. Mark before dense-source early continuation.
                if let Some(row) = self.subjects.get_mut(intermediate) {
                    row.mark_cross_group_incoming();
                }
                // A terminal subject has no outgoing row to subtract. Its
                // common-predecessor consequences remain grouped by this row.
                if !self.subjects.contains_key(intermediate) {
                    for target in &values {
                        pending.push(
                            data,
                            GeneralizedQuad::new(
                                intermediate.clone(),
                                predicate.clone(),
                                target.clone(),
                                GraphName::DefaultGraph,
                            ),
                        )?;
                    }
                }
                if dense {
                    continue;
                }
                // R R: only successors absent from the group's existing row can add facts.
                for successor in data.quads(
                    Some(intermediate.as_ref()),
                    Some(predicate.as_ref().into()),
                    None,
                ) {
                    let target = successor?.object;
                    if !values.contains(&target) {
                        missing.insert(target);
                    }
                }
            }
            for member in &group.members {
                for target in &missing {
                    pending.push(
                        data,
                        GeneralizedQuad::new(
                            member.clone(),
                            predicate.clone(),
                            target.clone(),
                            GraphName::DefaultGraph,
                        ),
                    )?;
                }
            }
        }
        self.extend_inverse(data, predicate, pending)
    }

    fn extend_inverse(
        &self,
        data: &impl Graph,
        predicate: &NamedNode,
        pending: &mut Pending,
    ) -> Result<()> {
        for (id, group) in self.groups.iter().enumerate() {
            // All representative outgoing rows have marked their actual source
            // witnesses. Closed groups need no second outgoing or incoming read.
            if self.components.is_dense(id)
                || group
                    .members
                    .iter()
                    .all(|member| !self.subjects[member].has_cross_group_incoming())
            {
                continue;
            }
            let values = row(data, &group.members[0], predicate)?;
            // First-phase rows/deltas have been dropped. Retain only this row
            // and its missing-target delta while evaluating source witnesses.
            let mut missing = HashSet::new();
            for member in &group.members {
                // Without a cross-group incoming edge, every predecessor has
                // exactly values as its outgoing row; no new head can exist.
                if !self.subjects[member].has_cross_group_incoming() {
                    continue;
                }
                missing.clear();
                for predecessor in
                    data.quads(None, Some(predicate.as_ref().into()), Some(member.as_ref()))
                {
                    let predecessor = predecessor?.subject;
                    let predecessor_group = self.subjects[&predecessor].group();
                    if predecessor_group == id
                        || predecessor != self.groups[predecessor_group].members[0]
                    {
                        continue;
                    }
                    // Identical predecessor rows all contain this member; their
                    // representative is therefore an actual source witness too.
                    for target in data.quads(
                        Some(predecessor.as_ref()),
                        Some(predicate.as_ref().into()),
                        None,
                    ) {
                        let target = target?.object;
                        if !values.contains(&target) {
                            missing.insert(target);
                        }
                    }
                }
                for target in &missing {
                    pending.push(
                        data,
                        GeneralizedQuad::new(
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
