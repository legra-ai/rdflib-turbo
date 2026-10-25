//! Typed, interned RDF term batches for the RDFLib/native boundary.

use std::collections::HashMap;

use pyo3::{IntoPyObjectExt, prelude::*};
use pyshacl_turbo_engine::oxigraph::model::*;
use pyshacl_turbo_engine::{Error, Result, serialize_graph};

type Record = (u8, String, Option<String>, Option<String>);
type QuadRecord = (usize, usize, usize, Option<usize>);
pub(crate) type Batch = (Vec<Record>, Vec<QuadRecord>);

pub(crate) enum Input {
    Terms(Batch),
    Text(String, super::InputFormat, Option<String>),
}

impl Input {
    pub fn read(source: &Bound<'_, PyAny>, format: &str, base: Option<&str>) -> PyResult<Self> {
        if format == "terms" {
            Ok(Self::Terms(source.extract()?))
        } else {
            Ok(Self::Text(
                source.extract()?,
                super::format(format)?,
                base.map(str::to_owned),
            ))
        }
    }

    pub fn parse(self) -> Result<Dataset> {
        match self {
            Self::Terms(batch) => decode(batch),
            Self::Text(source, format, base) => format.parse(&source, base.as_deref()),
        }
    }
}

pub(crate) enum Output {
    Terms(Batch),
    Text(String),
}

impl Output {
    pub fn graph(data: &Dataset, direct: bool) -> Result<Self> {
        if direct {
            Ok(Self::Terms(encode(data)))
        } else {
            Ok(Self::Text(serialize_graph(data)?))
        }
    }

    pub fn into_python(self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match self {
            Self::Terms(batch) => batch.into_py_any(py),
            Self::Text(text) => text.into_py_any(py),
        }
    }
}

/// An owned term table and lazy validated quads, without graph indexes.
pub(crate) struct DecodedTerms {
    terms: Vec<Term>,
    quads: std::vec::IntoIter<QuadRecord>,
}

impl DecodedTerms {
    pub(crate) fn new((records, quads): Batch) -> Result<Self> {
        let terms = records
            .into_iter()
            .map(|(kind, value, datatype, language)| {
                Ok(match (kind, datatype, language) {
                    (0, None, None) => NamedNode::new(value)
                        .map_err(|e| Error::Invalid(e.to_string()))?
                        .into(),
                    (1, None, None) => BlankNode::new(value)
                        .map_err(|e| Error::Invalid(e.to_string()))?
                        .into(),
                    (2, None, Some(language)) => {
                        Literal::new_language_tagged_literal(value, language)
                            .map_err(|e| Error::Invalid(e.to_string()))?
                            .into()
                    }
                    (2, datatype, None) => match datatype {
                        Some(datatype) => Literal::new_typed_literal(
                            value,
                            NamedNode::new(datatype).map_err(|e| Error::Invalid(e.to_string()))?,
                        )
                        .into(),
                        None => Literal::new_simple_literal(value).into(),
                    },
                    _ => return Err(Error::Invalid("Invalid RDF term record".into())),
                })
            })
            .collect::<Result<Vec<Term>>>()?;
        Ok(Self {
            terms,
            quads: quads.into_iter(),
        })
    }

    fn term(&self, index: usize) -> Result<Term> {
        self.terms
            .get(index)
            .cloned()
            .ok_or_else(|| Error::Invalid("Invalid RDF term index".into()))
    }

    fn quad(&self, (s, p, o, g): QuadRecord) -> Result<Quad> {
        let subject = match self.term(s)? {
            Term::NamedNode(node) => NamedOrBlankNode::NamedNode(node),
            Term::BlankNode(node) => NamedOrBlankNode::BlankNode(node),
            _ => {
                return Err(Error::Invalid(
                    "RDF subjects must be IRIs or blank nodes".into(),
                ));
            }
        };
        let Term::NamedNode(predicate) = self.term(p)? else {
            return Err(Error::Invalid("RDF predicates must be IRIs".into()));
        };
        let graph = match g.map(|index| self.term(index)).transpose()? {
            None => GraphName::DefaultGraph,
            Some(Term::NamedNode(node)) => GraphName::NamedNode(node),
            Some(Term::BlankNode(node)) => GraphName::BlankNode(node),
            _ => {
                return Err(Error::Invalid(
                    "RDF graph names must be IRIs or blank nodes".into(),
                ));
            }
        };
        Ok(Quad::new(subject, predicate, self.term(o)?, graph))
    }
}

impl Iterator for DecodedTerms {
    type Item = Result<Quad>;

    fn next(&mut self) -> Option<Self::Item> {
        self.quads.next().map(|record| self.quad(record))
    }
}

fn decode(batch: Batch) -> Result<Dataset> {
    let mut result = Dataset::new();
    for quad in DecodedTerms::new(batch)? {
        result.insert(quad?.as_ref());
    }
    Ok(result)
}

fn encode(data: &Dataset) -> Batch {
    let mut records = Vec::new();
    let mut indices = HashMap::new();
    let mut intern = |term: Term| {
        *indices.entry(term.clone()).or_insert_with(|| {
            let record = match term {
                Term::NamedNode(node) => (0, node.into_string(), None, None),
                Term::BlankNode(node) => (1, node.into_string(), None, None),
                Term::Literal(literal) => (
                    2,
                    literal.value().to_owned(),
                    (literal.language().is_none() && literal.has_explicit_datatype())
                        .then(|| literal.datatype().as_str().to_owned()),
                    literal.language().map(str::to_owned),
                ),
            };
            let index = records.len();
            records.push(record);
            index
        })
    };
    let quads = data
        .iter()
        .map(|q| {
            let subject = intern(q.subject.into_owned().into());
            let predicate = intern(q.predicate.into_owned().into());
            let object = intern(q.object.into_owned());
            let graph = match q.graph_name {
                GraphNameRef::DefaultGraph => None,
                GraphNameRef::NamedNode(node) => Some(intern(node.into_owned().into())),
                GraphNameRef::BlankNode(node) => Some(intern(node.into_owned().into())),
            };
            (subject, predicate, object, graph)
        })
        .collect();
    (records, quads)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoded_quads_validate_positions_lazily() {
        let records = vec![
            (0, "urn:s".into(), None, None),
            (0, "urn:p".into(), None, None),
            (2, "value".into(), None, None),
        ];
        let mut quads = DecodedTerms::new((
            records,
            vec![(0, 1, 2, None), (0, 1, 2, Some(2)), (99, 1, 2, None)],
        ))
        .unwrap();
        assert_eq!(
            quads.next().unwrap().unwrap().to_string(),
            "<urn:s> <urn:p> \"value\""
        );
        assert!(
            quads
                .next()
                .unwrap()
                .unwrap_err()
                .to_string()
                .contains("graph names")
        );
        assert!(
            quads
                .next()
                .unwrap()
                .unwrap_err()
                .to_string()
                .contains("term index")
        );
        assert!(quads.next().is_none());
    }

    #[test]
    fn rejects_malformed_records_and_indices() {
        assert!(decode((vec![(99, "invalid".into(), None, None)], vec![])).is_err());
        assert!(decode((vec![], vec![(0, 0, 0, None)])).is_err());
        let records = vec![
            (2, "literal".into(), None, None),
            (0, "urn:p".into(), None, None),
        ];
        assert!(decode((records.clone(), vec![(0, 1, 0, None)])).is_err());
        assert!(decode((records.clone(), vec![(1, 0, 0, None)])).is_err());
        assert!(decode((records, vec![(1, 1, 0, Some(0))])).is_err());
    }
}
