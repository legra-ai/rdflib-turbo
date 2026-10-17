use std::collections::BTreeMap;

use oxigraph::io::{RdfFormat, RdfSerializer};
use oxigraph::model::{Quad, Term};
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults, SparqlEvaluator};
use oxigraph::store::StorageError;
use serde::Serialize;

use super::operations::{invalid, runtime};
use crate::Result;

#[derive(Serialize)]
struct TermOutput<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    value: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    datatype: Option<&'a str>,
    #[serde(rename = "xml:lang", skip_serializing_if = "Option::is_none")]
    language: Option<&'a str>,
}

impl<'a> From<&'a Term> for TermOutput<'a> {
    fn from(term: &'a Term) -> Self {
        let (kind, value, datatype, language) = match term {
            Term::NamedNode(node) => ("uri", node.as_str(), None, None),
            Term::BlankNode(node) => ("bnode", node.as_str(), None, None),
            Term::Literal(literal) => (
                "literal",
                literal.value(),
                (literal.language().is_none() && literal.has_explicit_datatype())
                    .then(|| literal.datatype().as_str()),
                literal.language(),
            ),
        };
        Self {
            kind,
            value,
            datatype,
            language,
        }
    }
}

pub(super) fn prepare_query(
    source: &str,
    union: bool,
) -> Result<(PreparedSparqlQuery, &'static str)> {
    let parsed = spargebra::SparqlParser::new()
        .parse_query(source)
        .map_err(invalid)?;
    let kind = if matches!(parsed, spargebra::Query::Describe { .. }) {
        "DESCRIBE"
    } else {
        "CONSTRUCT"
    };
    let mut query = SparqlEvaluator::new().for_query(parsed);
    if union {
        query.dataset_mut().set_default_graph_as_union();
    }
    Ok((query, kind))
}

#[derive(Serialize)]
struct Solutions {
    variables: Vec<String>,
    rows: Vec<BTreeMap<String, OwnedTermOutput>>,
}

// A query explicitly requests materialized output, never a copy of the base store.
#[derive(Serialize)]
struct OwnedTermOutput {
    #[serde(rename = "type")]
    kind: &'static str,
    value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    datatype: Option<String>,
    #[serde(rename = "xml:lang", skip_serializing_if = "Option::is_none")]
    language: Option<String>,
}

impl From<&Term> for OwnedTermOutput {
    fn from(term: &Term) -> Self {
        let term = TermOutput::from(term);
        Self {
            kind: term.kind,
            value: term.value.into(),
            datatype: term.datatype.map(Into::into),
            language: term.language.map(Into::into),
        }
    }
}

pub(crate) fn query_output(
    results: QueryResults<'_>,
    graph_kind: &str,
) -> Result<(String, String)> {
    match results {
        QueryResults::Boolean(value) => Ok(("ASK".into(), value.to_string())),
        QueryResults::Solutions(solutions) => {
            let mut output = Solutions {
                variables: solutions
                    .variables()
                    .iter()
                    .map(|v| v.as_str().into())
                    .collect(),
                rows: Vec::new(),
            };
            for solution in solutions {
                let solution = solution?;
                output.rows.push(
                    solution
                        .iter()
                        .map(|(variable, term)| (variable.as_str().into(), term.into()))
                        .collect(),
                );
            }
            Ok((
                "SELECT".into(),
                serde_json::to_string(&output).map_err(runtime)?,
            ))
        }
        QueryResults::Graph(triples) => {
            let mut writer = RdfSerializer::from_format(RdfFormat::NQuads).for_writer(Vec::new());
            for triple in triples {
                writer.serialize_triple(&triple?).map_err(runtime)?;
            }
            Ok((
                graph_kind.into(),
                String::from_utf8(writer.finish().map_err(runtime)?).map_err(runtime)?,
            ))
        }
    }
}

pub(super) fn dump(
    quads: impl Iterator<Item = std::result::Result<Quad, StorageError>>,
    format: RdfFormat,
) -> Result<String> {
    let mut writer = RdfSerializer::from_format(format).for_writer(Vec::new());
    for quad in quads {
        writer
            .serialize_quad(&quad.map_err(runtime)?)
            .map_err(invalid)?;
    }
    String::from_utf8(writer.finish().map_err(runtime)?).map_err(runtime)
}
