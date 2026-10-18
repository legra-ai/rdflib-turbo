//! Exact term transport for RDFLib Store operations, without RDF document copies.
use oxigraph::model::{
    BlankNode, GraphName, GraphNameRef, Literal, NamedNode, NamedOrBlankNode, Quad, Term,
};
use oxigraph::store::{QuadIter, StorageError};
use serde::Deserialize;

use super::operations::{invalid, runtime};
use super::output::OwnedTermOutput;
use crate::{Error, Result};

pub(super) struct Pattern {
    pub subject: Option<NamedOrBlankNode>,
    pub predicate: Option<NamedNode>,
    pub object: Option<Term>,
    pub graph: Option<GraphName>,
}

#[derive(Deserialize)]
struct InputTerm {
    #[serde(rename = "type")]
    kind: String,
    value: String,
    datatype: Option<String>,
    #[serde(rename = "xml:lang")]
    language: Option<String>,
}

impl InputTerm {
    fn term(self) -> Result<Term> {
        // The Python boundary already supplies RDFLib terms. Do not reinterpret
        // their spelling through an RDF parser (RDFLib permits relative URIRefs).
        match self.kind.as_str() {
            "uri" => Ok(NamedNode::new_unchecked(self.value).into()),
            "bnode" => Ok(BlankNode::new_unchecked(self.value).into()),
            "literal" => Ok(if let Some(language) = self.language {
                Literal::new_language_tagged_literal(self.value, language).map_err(invalid)?
            } else if let Some(datatype) = self.datatype {
                Literal::new_typed_literal(self.value, NamedNode::new_unchecked(datatype))
            } else {
                Literal::new_simple_literal(self.value)
            }
            .into()),
            _ => Err(Error::Invalid("Unknown RDF term type".into())),
        }
    }
}

pub(super) fn graph(source: &str) -> Result<GraphName> {
    if source.is_empty() {
        return Ok(GraphName::DefaultGraph);
    }
    match serde_json::from_str::<InputTerm>(source)
        .map_err(invalid)?
        .term()?
    {
        Term::NamedNode(node) => Ok(node.into()),
        Term::BlankNode(node) => Ok(node.into()),
        _ => Err(Error::Invalid(
            "Graph name must be an IRI or blank node".into(),
        )),
    }
}

impl Pattern {
    pub fn count<'a>(
        &'a self,
        graph_len: impl FnOnce(GraphNameRef<'a>) -> Result<usize>,
        quads: impl FnOnce() -> QuadIter<'a>,
    ) -> Result<usize> {
        if self.subject.is_none()
            && self.predicate.is_none()
            && self.object.is_none()
            && let Some(graph) = &self.graph
        {
            return graph_len(graph.as_ref());
        }
        count(quads(), self.graph.is_none())
    }

    pub fn parse(source: &str) -> Result<Self> {
        let (subject, predicate, object, graph_name): (
            Option<InputTerm>,
            Option<InputTerm>,
            Option<InputTerm>,
            Option<String>,
        ) = serde_json::from_str(source).map_err(invalid)?;
        Ok(Self {
            subject: subject
                .map(|source| match source.term()? {
                    Term::NamedNode(node) => Ok(node.into()),
                    Term::BlankNode(node) => Ok(node.into()),
                    _ => Err(Error::Invalid(
                        "Subject must be an IRI or blank node".into(),
                    )),
                })
                .transpose()?,
            predicate: predicate
                .map(|source| match source.term()? {
                    Term::NamedNode(node) => Ok(node),
                    _ => Err(Error::Invalid("Predicate must be an IRI".into())),
                })
                .transpose()?,
            object: object.map(InputTerm::term).transpose()?,
            graph: graph_name.map(|source| graph(&source)).transpose()?,
        })
    }
}

pub(super) fn output(
    quads: impl Iterator<Item = std::result::Result<Quad, StorageError>>,
) -> Result<String> {
    let mut rows = Vec::new();
    for quad in quads {
        let quad = quad.map_err(runtime)?;
        let subject: Term = quad.subject.into();
        let predicate: Term = quad.predicate.into();
        let graph: Option<Term> = match quad.graph_name {
            GraphName::DefaultGraph => None,
            GraphName::NamedNode(node) => Some(node.into()),
            GraphName::BlankNode(node) => Some(node.into()),
        };
        rows.push((
            OwnedTermOutput::from(&subject),
            OwnedTermOutput::from(&predicate),
            OwnedTermOutput::from(&quad.object),
            graph.as_ref().map(OwnedTermOutput::from),
        ));
    }
    serde_json::to_string(&rows).map_err(runtime)
}

pub(super) fn count(quads: QuadIter<'_>, union: bool) -> Result<usize> {
    if union {
        let mut triples = std::collections::HashSet::new();
        for quad in quads {
            let quad = quad.map_err(runtime)?;
            triples.insert((quad.subject, quad.predicate, quad.object));
        }
        Ok(triples.len())
    } else {
        let mut count = 0;
        // A fixed graph has no duplicate SPO rows: count encoded matches without decoding terms.
        for row in quads.project(false, false, false) {
            row.map_err(runtime)?;
            count += 1;
        }
        Ok(count)
    }
}

#[cfg(test)]
#[path = "count_tests.rs"]
mod count_tests;
