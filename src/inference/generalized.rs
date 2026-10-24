//! Indexed non-RDF tuples retained between shared-rule rounds, never published.

use oxigraph::model::*;
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults};

use super::Pending;
use super::backend::Graph;
use crate::{Error, Result};

mod equality;
mod query;
pub(super) use equality::EqualityRules;
#[cfg(test)]
mod equality_tests;
#[cfg(test)]
mod equality_work_tests;
#[cfg(test)]
mod tests;

pub(super) const GENERALIZED_GRAPH: NamedNodeRef<'static> =
    NamedNodeRef::new_unchecked("urn:rdflib-turbo:inference:generalized");

const COLUMNS: [NamedNodeRef<'static>; 3] = [
    NamedNodeRef::new_unchecked("urn:rdflib-turbo:inference:subject"),
    NamedNodeRef::new_unchecked("urn:rdflib-turbo:inference:predicate"),
    NamedNodeRef::new_unchecked("urn:rdflib-turbo:inference:object"),
];

/// Only the non-RDF inference delta is stored, using the native indexed dataset.
/// Each row owns three column values; the input graph is borrowed, never copied.
#[derive(Default)]
pub(super) struct Generalized {
    rows: Dataset,
}

impl Generalized {
    fn matching<'a>(
        &'a self,
        pattern: [Option<TermRef<'_>>; 3],
    ) -> impl Iterator<Item = [TermRef<'a>; 3]> + use<'a> {
        // bounded: exactly three selector values, independent of relation size.
        let pattern = pattern.map(|term| term.map(TermRef::into_owned));
        // Choose an object-indexed column when bound; otherwise scan row subjects.
        let column = pattern.iter().position(Option::is_some).unwrap_or(0);
        self.rows
            .quads_for_pattern(
                None,
                Some(COLUMNS[column]),
                pattern[column].as_ref().map(Term::as_ref),
                None,
            )
            .filter_map(move |quad| {
                let row = std::array::from_fn(|index| {
                    self.rows
                        .quads_for_pattern(Some(quad.subject), Some(COLUMNS[index]), None, None)
                        .next()
                        .expect("generalized row must have all three columns")
                        .object
                });
                pattern
                    .iter()
                    .zip(row)
                    .all(|(bound, term)| bound.as_ref().is_none_or(|bound| bound.as_ref() == term))
                    .then_some(row)
            })
    }

    fn contains(&self, row: &[Term; 3]) -> bool {
        self.matching(row.each_ref().map(|term| Some(term.as_ref())))
            .next()
            .is_some()
    }

    fn insert(&mut self, row: [Term; 3]) {
        if self.contains(&row) {
            return;
        }
        let subject = BlankNode::default();
        for (column, value) in COLUMNS.into_iter().zip(row) {
            self.rows.insert(&Quad::new(
                subject.clone(),
                column,
                value,
                GraphName::DefaultGraph,
            ));
        }
    }

    pub(super) fn extend(&mut self, pending: Self) -> bool {
        let changed = !pending.rows.is_empty();
        for quad in &pending.rows {
            self.rows.insert(quad);
        }
        changed
    }

    pub(super) fn apply(
        &self,
        data: &impl Graph,
        queries: &[PreparedSparqlQuery],
        ordinary: &mut Pending,
    ) -> Result<Self> {
        let mut pending = Self::default();
        for query in queries {
            // Preserve the backend's borrowed query path until generalized rows exist.
            let results = if self.rows.is_empty() {
                data.query(query.clone())?
            } else {
                query
                    .clone()
                    .on_queryable_dataset(query::View { data, extra: self })
                    .execute()?
            };
            self.retain(results, data, ordinary, &mut pending)?;
        }
        Ok(pending)
    }

    fn retain(
        &self,
        results: QueryResults<'_>,
        data: &impl Graph,
        ordinary: &mut Pending,
        pending: &mut Self,
    ) -> Result<()> {
        let QueryResults::Solutions(solutions) = results else {
            return Err(Error::Runtime(
                "Shared inference requires head bindings".into(),
            ));
        };
        for solution in solutions {
            let solution = solution?;
            let row = ["__head_s", "__head_p", "__head_o"].map(|name| {
                solution
                    .get(name)
                    .cloned()
                    .ok_or_else(|| Error::Runtime(format!("Unbound shared inference head: {name}")))
            });
            let [subject, predicate, object] = row;
            let row = [subject?, predicate?, object?];
            match (
                NamedOrBlankNode::try_from(row[0].clone()),
                NamedNode::try_from(row[1].clone()),
            ) {
                (Ok(subject), Ok(predicate)) => ordinary.push(
                    data,
                    Quad::new(subject, predicate, row[2].clone(), GraphName::DefaultGraph),
                )?,
                _ if !self.contains(&row) => pending.insert(row),
                _ => {}
            }
        }
        Ok(())
    }
}
