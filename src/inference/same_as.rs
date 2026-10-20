//! One-step equality compositions over one immutable inference-round snapshot.
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};

use oxigraph::model::*;

use super::{Graph, OWL, Pending};
use crate::Result;

#[path = "same_as_components.rs"]
mod components;
#[path = "same_as_dense.rs"]
mod dense;
use components::Components;

struct RowGroup {
    members: Vec<NamedOrBlankNode>,
    size: usize,
    dense: bool,
}

struct Rows {
    subjects: HashMap<NamedOrBlankNode, usize>,
    groups: Vec<RowGroup>,
    components: Components,
}

fn row(
    data: &impl Graph,
    subject: &NamedOrBlankNode,
    predicate: &NamedNode,
) -> Result<HashSet<Term>> {
    data.quads(Some(subject.as_ref()), Some(predicate.as_ref()), None)
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
    fn new(data: &impl Graph, predicate: &NamedNode, pending: &mut Pending) -> Result<Self> {
        // Transient O(subjects) identities/groups, never a retained copy of all equality edges.
        let mut output = Self {
            subjects: HashMap::new(),
            groups: Vec::new(),
            components: Components::default(),
        };
        let mut signatures: HashMap<(usize, u64), Vec<usize>> = HashMap::new();
        for quad in data.quads(None, Some(predicate.as_ref()), None) {
            let quad = quad?;
            output
                .components
                .connect(quad.subject.clone().into(), quad.object.clone());
            if let Ok(inverse_subject) = NamedOrBlankNode::try_from(quad.object) {
                pending.push(
                    data,
                    Quad::new(
                        inverse_subject,
                        predicate.clone(),
                        Term::from(quad.subject.clone()),
                        GraphName::DefaultGraph,
                    ),
                )?;
            }
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
                    Some(predicate.as_ref()),
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
                    size: values.len(),
                    dense: false,
                });
                signatures.entry(signature).or_default().push(index);
                index
            });
            output.groups[group].members.push(quad.subject.clone());
            output.subjects.insert(quad.subject, group);
        }
        output.components.finish();
        for group in &mut output.groups {
            group.dense = group.size
                > output
                    .components
                    .members(&group.members[0].clone().into())
                    .len()
                    / 2;
        }
        Ok(output)
    }

    fn extend(
        &self,
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
                let Ok(intermediate) = NamedOrBlankNode::try_from(intermediate.clone()) else {
                    continue;
                };
                if self.subjects.get(&intermediate) == Some(&id) {
                    continue;
                }
                // R^-1 R: subject replacement on an equality predicate. The inverse
                // edge itself is pending and must not become visible to this pass.
                if !self.dense(&intermediate) {
                    for target in &values {
                        pending.push(
                            data,
                            Quad::new(
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
                for successor in
                    data.quads(Some(intermediate.as_ref()), Some(predicate.as_ref()), None)
                {
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

fn replacements(data: &impl Graph, predicate: &NamedNode, pending: &mut Pending) -> Result<()> {
    for quad in data.quads(None, None, None) {
        let quad = quad?;
        // Equality-predicate consequences are R R / R^-1 R above. Reject these
        // before alias lookups, rather than filtering a cubic SPARQL witness bag.
        if quad.predicate == *predicate {
            continue;
        }
        for alias in data.quads(Some(quad.subject.as_ref()), Some(predicate.as_ref()), None) {
            if let Ok(subject) = NamedOrBlankNode::try_from(alias?.object) {
                pending.push(
                    data,
                    Quad::new(
                        subject,
                        quad.predicate.clone(),
                        quad.object.clone(),
                        GraphName::DefaultGraph,
                    ),
                )?;
            }
        }
        if let Ok(object) = NamedOrBlankNode::try_from(quad.object.clone()) {
            for alias in data.quads(Some(object.as_ref()), Some(predicate.as_ref()), None) {
                pending.push(
                    data,
                    Quad::new(
                        quad.subject.clone(),
                        quad.predicate.clone(),
                        alias?.object,
                        GraphName::DefaultGraph,
                    ),
                )?;
            }
        }
    }
    Ok(())
}

pub(super) fn apply(data: &impl Graph, pending: &mut Pending) -> Result<()> {
    let predicate = NamedNode::new_unchecked(format!("{OWL}sameAs"));
    let rows = Rows::new(data, &predicate, pending)?;
    if rows.groups.is_empty() {
        return Ok(());
    }
    rows.extend(data, &predicate, pending)?;
    replacements(data, &predicate, pending)
}
