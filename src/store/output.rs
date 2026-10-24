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
    rdflib_context: bool,
) -> Result<(PreparedSparqlQuery, &'static str)> {
    let parser = spargebra::SparqlParser::new();
    let parser = if rdflib_context {
        parser.with_rdflib_query_context()
    } else {
        parser
    };
    let parsed = parser.parse_query(source).map_err(invalid)?;
    let kind = if matches!(parsed, spargebra::Query::Describe { .. }) {
        "DESCRIBE"
    } else {
        "CONSTRUCT"
    };
    let mut query = SparqlEvaluator::new().for_query(parsed);
    if union && query.dataset().is_default_dataset() {
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
pub(super) struct OwnedTermOutput {
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

pub(super) use super::nt_format::{GraphNtOutput, dump_graph_nt};

#[cfg(test)]
mod nt_serialization_tests {
    use super::*;
    use oxigraph::model::{BlankNode, GeneralizedQuad, GraphName, Literal, NamedNode};
    use oxigraph::store::Store;

    #[test]
    fn generalized_subject_and_predicate_use_reference_n3_roles() {
        let terms: [Term; 3] = [
            NamedNode::new("urn:node").unwrap().into(),
            BlankNode::new("node").unwrap().into(),
            Literal::new_simple_literal("line\nend").into(),
        ];
        for subject in &terms {
            for predicate in &terms {
                let store = Store::new().unwrap();
                store
                    .insert_generalized(&GeneralizedQuad::new(
                        subject.clone(),
                        predicate.clone(),
                        Literal::new_simple_literal("object"),
                        GraphName::DefaultGraph,
                    ))
                    .unwrap();
                let output =
                    dump_graph_nt(store.generalized_quads_for_pattern(None, None, None, None));
                assert!(
                    matches!(output, Ok(GraphNtOutput::Document(_))),
                    "All RDFLib-accepted subject/predicate roles must serialize"
                );
            }
        }
    }

    #[test]
    fn selected_physical_graph_keeps_exact_literal_and_blank_identity() {
        for graph in [
            GraphName::DefaultGraph,
            NamedNode::new("urn:selected").unwrap().into(),
            BlankNode::new("selected").unwrap().into(),
        ] {
            let store = Store::new().unwrap();
            let subject = BlankNode::new("subject-id").unwrap();
            let predicate = NamedNode::new("urn:p").unwrap();
            let values = [
                Literal::new_simple_literal("same"),
                Literal::new_typed_literal(
                    "same",
                    NamedNode::new("http://www.w3.org/2001/XMLSchema#string").unwrap(),
                ),
                Literal::new_language_tagged_literal("texte", "fr").unwrap(),
                Literal::new_simple_literal("漢字🌍\n\t\r\\\"".repeat(512)),
            ];
            for value in &values {
                store
                    .insert(&Quad::new(
                        subject.clone(),
                        predicate.clone(),
                        value.clone(),
                        graph.clone(),
                    ))
                    .unwrap();
            }
            store
                .insert(&Quad::new(
                    NamedNode::new("urn:exclude").unwrap(),
                    predicate,
                    Literal::new_simple_literal("outside"),
                    NamedNode::new("urn:other").unwrap(),
                ))
                .unwrap();
            let GraphNtOutput::Document(text) = dump_graph_nt(store.generalized_quads_for_pattern(
                None,
                None,
                None,
                Some(graph.as_ref()),
            ))
            .unwrap() else {
                panic!("Valid triple terms must serialize");
            };
            assert_eq!(text.lines().count(), values.len());
            assert!(text.contains("_:subject-id"));
            assert!(text.contains("\"same\" .\n"));
            assert!(text.contains("\"same\"^^<http://www.w3.org/2001/XMLSchema#string>"));
            assert!(text.contains("\"texte\"@fr"));
            assert!(text.contains("漢字🌍"));
            assert!(!text.contains("urn:exclude"));
        }
    }

    #[test]
    fn selected_graph_writer_sees_pending_transaction_without_publication() {
        let store = Store::new().unwrap();
        let mut transaction = store.start_transaction().unwrap();
        transaction.insert(&Quad::new(
            NamedNode::new("urn:s").unwrap(),
            NamedNode::new("urn:p").unwrap(),
            Literal::new_simple_literal("pending"),
            GraphName::DefaultGraph,
        ));
        let GraphNtOutput::Document(text) =
            dump_graph_nt(transaction.generalized_quads_for_pattern(
                None,
                None,
                None,
                Some(GraphName::DefaultGraph.as_ref()),
            ))
            .unwrap()
        else {
            panic!("Valid pending triple must serialize");
        };
        assert_eq!(text, "<urn:s> <urn:p> \"pending\" .\n");
        assert!(
            matches!(dump_graph_nt(store.generalized_quads_for_pattern(None, None, None, None)).unwrap(), GraphNtOutput::Document(text) if text.is_empty())
        );
        drop(transaction);
        assert!(
            matches!(dump_graph_nt(store.generalized_quads_for_pattern(None, None, None, None)).unwrap(), GraphNtOutput::Document(text) if text.is_empty())
        );
    }
}
