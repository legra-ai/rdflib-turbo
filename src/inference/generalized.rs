//! Indexed non-RDF tuples retained between shared-rule rounds, never published.

use oxigraph::model::*;
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults};

use super::Pending;
use super::backend::Graph;
use crate::{Error, Result};

mod equality;
mod query;
mod tuples;
pub(super) use equality::EqualityRules;
pub(super) use tuples::Tuples;
#[cfg(test)]
mod diagnostic_tests;
#[cfg(test)]
mod equality_tests;
#[cfg(test)]
mod equality_work_tests;
#[cfg(test)]
mod list_cardinality_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tuple_tests;

pub(super) const GENERALIZED_GRAPH: NamedNodeRef<'static> =
    NamedNodeRef::new_unchecked("urn:rdflib-turbo:inference:generalized");

const REFLEXIVE_SUBJECT: NamedNodeRef<'static> =
    NamedNodeRef::new_unchecked("urn:rdflib-turbo:inference:literal-reflexivity");
const SAME_AS: NamedNodeRef<'static> =
    NamedNodeRef::new_unchecked("http://www.w3.org/2002/07/owl#sameAs");

const LITERAL_TYPES: NamedNodeRef<'static> =
    NamedNodeRef::new_unchecked("urn:rdflib-turbo:inference:literal-type");
const RDF_TYPE: NamedNodeRef<'static> =
    NamedNodeRef::new_unchecked("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");

const COLUMNS: [NamedNodeRef<'static>; 3] = [
    NamedNodeRef::new_unchecked("urn:rdflib-turbo:inference:subject"),
    NamedNodeRef::new_unchecked("urn:rdflib-turbo:inference:predicate"),
    NamedNodeRef::new_unchecked("urn:rdflib-turbo:inference:object"),
];

/// Only the non-RDF inference delta is stored, using the native indexed dataset.
/// Literal reflexivity and type membership use compact indexed records; other
/// tuples use three columns. The input graph is borrowed, never copied.
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
        let explicit = self
            .rows
            .quads_for_pattern(
                None,
                Some(COLUMNS[column]),
                pattern[column].as_ref().map(Term::as_ref),
                None,
            )
            .map(move |quad| {
                std::array::from_fn(|index| {
                    self.rows
                        .quads_for_pattern(Some(quad.subject), Some(COLUMNS[index]), None, None)
                        .next()
                        .expect("generalized row must have all three columns")
                        .object
                })
            });
        // Literal reflexivity has one independent value. Keep one indexed record
        // per value and reconstruct the same tuple for every query shape.
        let reflexive_enabled = pattern[1]
            .as_ref()
            .is_none_or(|term| term.as_ref() == TermRef::from(SAME_AS));
        let reflexive_value = pattern[0]
            .as_ref()
            .or(pattern[2].as_ref())
            .map(Term::as_ref);
        let reflexive = reflexive_enabled
            .then(|| {
                self.rows.quads_for_pattern(
                    Some(REFLEXIVE_SUBJECT.into()),
                    Some(SAME_AS),
                    reflexive_value,
                    None,
                )
            })
            .into_iter()
            .flatten()
            .map(|quad| [quad.object, SAME_AS.into(), quad.object]);
        // A literal type has two independent values. Invert them into an RDF
        // record, so the native subject/object indexes serve either bound slot.
        let type_subject = pattern[2].as_ref().and_then(|term| match term {
            Term::NamedNode(node) => Some(node.as_ref().into()),
            Term::BlankNode(node) => Some(node.as_ref().into()),
            Term::Literal(_) => None,
        });
        let types_enabled = pattern[1]
            .as_ref()
            .is_none_or(|term| term.as_ref() == TermRef::from(RDF_TYPE))
            && (pattern[2].is_none() || type_subject.is_some());
        let types = types_enabled
            .then(|| {
                self.rows.quads_for_pattern(
                    type_subject,
                    Some(LITERAL_TYPES),
                    pattern[0].as_ref().map(Term::as_ref),
                    None,
                )
            })
            .into_iter()
            .flatten()
            .map(|quad| [quad.object, RDF_TYPE.into(), quad.subject.into()]);
        explicit.chain(reflexive).chain(types).filter(move |row| {
            pattern
                .iter()
                .zip(*row)
                .all(|(bound, term)| bound.as_ref().is_none_or(|bound| bound.as_ref() == term))
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
        if row[0].is_literal() && row[0] == row[2] && row[1].as_ref() == TermRef::from(SAME_AS) {
            self.rows.insert(QuadRef::new(
                REFLEXIVE_SUBJECT,
                SAME_AS,
                row[0].as_ref(),
                GraphNameRef::DefaultGraph,
            ));
            return;
        }
        if row[0].is_literal()
            && row[1].as_ref() == TermRef::from(RDF_TYPE)
            && let Ok(class) = NamedOrBlankNode::try_from(row[2].clone())
        {
            self.rows.insert(&Quad::new(
                class,
                LITERAL_TYPES,
                row[0].clone(),
                GraphName::DefaultGraph,
            ));
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

    pub(super) fn literal_type(&mut self, literal: &Literal, datatype: NamedNode) {
        self.insert([
            literal.clone().into(),
            crate::graph::iri(crate::graph::RDF, "type").into(),
            datatype.into(),
        ]);
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
            self.retain_row(row, data, ordinary, pending)?;
        }
        Ok(())
    }

    fn retain_row(
        &self,
        row: [Term; 3],
        data: &impl Graph,
        ordinary: &mut Pending,
        pending: &mut Self,
    ) -> Result<()> {
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
        Ok(())
    }
}
