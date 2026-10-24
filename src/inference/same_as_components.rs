//! Explore only small weak components where missing-pair search can save work.
use std::collections::{HashMap, HashSet, VecDeque};

use oxigraph::model::*;

use super::{Graph, Rows, row};
use crate::Result;

#[cfg(test)]
#[path = "same_as_planning_tests.rs"]
mod tests;

#[derive(Default)]
pub(super) struct Components {
    dense: HashMap<usize, usize>,
    members: Vec<Vec<Term>>,
}

fn group(rows: &Rows, term: &Term) -> Option<usize> {
    NamedOrBlankNode::try_from(term.clone())
        .ok()
        .and_then(|subject| rows.subjects.get(&subject).map(|row| row.group()))
}

fn explore(
    data: &impl Graph,
    predicate: &NamedNode,
    seed: &NamedOrBlankNode,
    limit: usize,
) -> Result<(HashSet<Term>, bool)> {
    // bounded: at most twice the seed's outgoing degree plus one overflow term.
    let mut seen = HashSet::from([Term::from(seed.clone())]);
    let mut frontier = VecDeque::from([Term::from(seed.clone())]);
    while let Some(term) = frontier.pop_front() {
        if let Ok(subject) = NamedOrBlankNode::try_from(term.clone()) {
            for quad in data.quads(Some(subject.as_ref()), Some(predicate.as_ref()), None) {
                let target = quad?.object;
                if seen.insert(target.clone()) {
                    if seen.len() > limit {
                        return Ok((seen, false));
                    }
                    frontier.push_back(target);
                }
            }
        }
        for quad in data.quads(None, Some(predicate.as_ref()), Some(term.as_ref())) {
            let source = Term::from(quad?.subject);
            if seen.insert(source.clone()) {
                if seen.len() > limit {
                    return Ok((seen, false));
                }
                frontier.push_back(source);
            }
        }
    }
    Ok((seen, true))
}

impl Components {
    pub(super) fn discover(
        data: &impl Graph,
        predicate: &NamedNode,
        rows: &Rows,
        signatures: &HashMap<(usize, u64), Vec<usize>>,
    ) -> Result<Self> {
        let mut output = Self::default();
        // Balanced groups already bound repeated witnesses by their input/output.
        // Keep only unbalanced seed identities; balanced-only graphs allocate nothing.
        let seeds: HashMap<usize, usize> = signatures
            .iter()
            .flat_map(|(&(degree, _), groups)| {
                groups.iter().filter_map(move |&id| {
                    (degree > rows.groups[id].members.len()).then_some((id, degree))
                })
            })
            .collect();
        let mut resolved = HashSet::new();
        for (&seed, &degree) in &seeds {
            if resolved.contains(&seed) {
                continue;
            }
            let (terms, complete) = explore(
                data,
                predicate,
                &rows.groups[seed].members[0],
                degree.saturating_mul(2),
            )?;
            if !complete {
                // The component exceeds twice this degree. Any encountered seed
                // of no greater degree also cannot have a dense outgoing row.
                for term in &terms {
                    if let Some(id) = group(rows, term)
                        && seeds.get(&id).is_some_and(|&other| other <= degree)
                    {
                        resolved.insert(id);
                    }
                }
                continue;
            }
            let mut groups = HashSet::new();
            for term in &terms {
                if let Some(id) = group(rows, term) {
                    groups.insert(id);
                }
            }
            let component = output.members.len();
            let mut retained = false;
            for id in groups {
                let degree = match seeds.get(&id) {
                    Some(&degree) => degree,
                    None => row(data, &rows.groups[id].members[0], predicate)?.len(),
                };
                if degree > terms.len() / 2 {
                    output.dense.insert(id, component);
                    retained = true;
                }
                resolved.insert(id);
            }
            if retained {
                output.members.push(terms.into_iter().collect());
            }
        }
        Ok(output)
    }

    pub(super) fn is_dense(&self, group: usize) -> bool {
        self.dense.contains_key(&group)
    }

    pub(super) fn members(&self, group: usize) -> &[Term] {
        &self.members[self.dense[&group]]
    }
}
