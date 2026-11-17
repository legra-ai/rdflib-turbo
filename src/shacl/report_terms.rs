//! Compact term records that carry SHACL report graphs across the Python boundary.
use std::collections::HashMap;

use oxigraph::model::{Dataset, GraphNameRef, TermRef};

pub(super) type Record = (u8, String, Option<String>, Option<String>);
pub(super) type QuadRecord = (usize, usize, usize, Option<usize>);
pub(super) type Batch = (Vec<Record>, Vec<QuadRecord>);
pub(crate) type Output = (bool, Batch, String, Vec<String>);

#[derive(Default)]
pub(super) struct ReportTerms<'a> {
    records: Vec<Record>,
    indices: HashMap<TermRef<'a>, usize>,
}

impl<'a> ReportTerms<'a> {
    pub(super) fn encode(data: &'a Dataset) -> Batch {
        let mut terms = Self::default();
        let quads = data
            .iter()
            .map(|quad| {
                let subject = terms.intern(quad.subject.into());
                let predicate = terms.intern(quad.predicate.into());
                let object = terms.intern(quad.object);
                let graph = match quad.graph_name {
                    GraphNameRef::DefaultGraph => None,
                    GraphNameRef::NamedNode(node) => Some(terms.intern(node.into())),
                    GraphNameRef::BlankNode(node) => Some(terms.intern(node.into())),
                };
                (subject, predicate, object, graph)
            })
            .collect();
        (terms.records, quads)
    }

    fn intern(&mut self, term: TermRef<'a>) -> usize {
        if let Some(&index) = self.indices.get(&term) {
            return index;
        }
        let record = match term {
            TermRef::NamedNode(node) => (0, node.as_str().to_owned(), None, None),
            TermRef::BlankNode(node) => (1, node.as_str().to_owned(), None, None),
            TermRef::Literal(literal) => (
                2,
                literal.value().to_owned(),
                (literal.language().is_none() && literal.has_explicit_datatype())
                    .then(|| literal.datatype().as_str().to_owned()),
                literal.language().map(str::to_owned),
            ),
        };
        let index = self.records.len();
        self.records.push(record);
        self.indices.insert(term, index);
        index
    }
}

#[cfg(test)]
#[path = "report_terms_tests.rs"]
mod tests;
