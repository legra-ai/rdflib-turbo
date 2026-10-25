//! Exact term transport for RDFLib Store operations, without RDF document copies.
use oxigraph::model::{GeneralizedQuad, GraphName, GraphNameRef, Term};
use oxigraph::store::{GeneralizedQuadIter, StorageError};
use serde::Deserialize;

use super::operations::{invalid, runtime};
use super::output::OwnedTermOutput;
use crate::{Error, Result};

pub(super) struct Pattern {
    pub subject: Option<Term>,
    pub predicate: Option<Term>,
    pub object: Option<Term>,
    pub graph: Option<GraphName>,
}

#[derive(Deserialize)]
pub(super) struct InputTerm {
    #[serde(rename = "type")]
    kind: String,
    value: String,
    datatype: Option<String>,
    #[serde(rename = "xml:lang")]
    language: Option<String>,
}

impl InputTerm {
    pub(super) fn blank_node_label(&self) -> Option<&str> {
        (self.kind == "bnode").then_some(self.value.as_str())
    }

    pub(super) fn term(self) -> Result<Term> {
        super::typed::term((self.kind, self.value, self.datatype, self.language))
    }
}

pub(crate) fn graph(source: &str) -> Result<GraphName> {
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
        quads: impl FnOnce() -> GeneralizedQuadIter<'a>,
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
            subject: subject.map(InputTerm::term).transpose()?,
            predicate: predicate.map(InputTerm::term).transpose()?,
            object: object.map(InputTerm::term).transpose()?,
            graph: graph_name.map(|source| graph(&source)).transpose()?,
        })
    }
}

pub(super) fn output(
    quads: impl Iterator<Item = std::result::Result<GeneralizedQuad, StorageError>>,
) -> Result<String> {
    let mut rows = Vec::new();
    for quad in quads {
        let quad = quad.map_err(runtime)?;
        let subject = quad.subject;
        let predicate = quad.predicate;
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

pub(super) fn count(quads: GeneralizedQuadIter<'_>, union: bool) -> Result<usize> {
    let quads = if union { quads.unique_spo() } else { quads };
    let mut count = 0;
    // Count encoded representatives without lexical decoding or a retained SPO set.
    // The selector and its exact representative probes share this same reader.
    for row in quads.project_lexical(false, false, false) {
        row.map_err(runtime)?;
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
#[path = "count_tests.rs"]
mod count_tests;
