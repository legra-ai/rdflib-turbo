use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::Dataset;
use oxigraph::sparql::SparqlEvaluator;
use oxigraph::store::Transaction;
use pyshacl_turbo_engine::{CompiledShapes, Options};
use std::sync::Arc;

use crate::{Error, Result};

use super::output::{dump, prepare_query, query_output};
use super::pattern::{self, Pattern};
use super::typed::{self, QuadInput, TermTuple};

pub(super) enum Operation {
    Validate {
        shapes: Arc<CompiledShapes>,
        options: Options,
        inplace: bool,
    },
    InsertTerms(Box<QuadInput>),
    Remove(String),
    Match(String),
    Count(String),
    QueryGraph(String, String),
    LoadGraph {
        source: String,
        format: RdfFormat,
        base: Option<String>,
        graph: String,
        allow_named_graphs: bool,
    },
    Graphs,
    ContainsGraph(TermTuple),
    GraphLen(Option<TermTuple>),
    Graph(String, bool),
    Len,
    Load {
        source: String,
        format: RdfFormat,
        base: Option<String>,
    },
    Update(String),
    Infer(String, Option<String>, crate::inference::InferenceOptions),
    Query(String, bool),
    Dump(RdfFormat),
}

pub(super) enum Reply {
    Validation(Box<crate::shacl::Report>),
    StagedValidation(Box<(crate::shacl::Report, Dataset)>),
    PreparedValidation(Box<(crate::shacl::Report, u64)>),
    Done,
    Bool(bool),
    Count(usize),
    Text(String),
    Query(String, String),
}

impl Operation {
    // Exhaustive classification: adding an operation requires an explicit mutation policy.
    pub fn mutates(&self) -> bool {
        match self {
            Self::InsertTerms(_)
            | Self::Remove(_)
            | Self::LoadGraph { .. }
            | Self::Graph(_, _)
            | Self::Load { .. }
            | Self::Update(_)
            | Self::Infer(..) => true,
            Self::Validate { .. }
            | Self::Match(_)
            | Self::Count(_)
            | Self::QueryGraph(..)
            | Self::Graphs
            | Self::ContainsGraph(_)
            | Self::GraphLen(_)
            | Self::Len
            | Self::Query(..)
            | Self::Dump(_) => false,
        }
    }

    pub fn execute(self, transaction: &mut Transaction<'_>) -> Result<Reply> {
        match self {
            Self::Validate {
                shapes,
                options,
                inplace,
            } => {
                let outcome = shapes.validate_delta(&*transaction, options)?;
                let report = crate::shacl::Report::try_new(outcome.report)?;
                let additions = if inplace {
                    outcome.additions
                } else {
                    Dataset::new()
                };
                Ok(Reply::StagedValidation(Box::new((report, additions))))
            }
            Self::Count(source) => {
                let pattern = Pattern::parse(&source)?;
                pattern
                    .count(
                        |graph| transaction.graph_len(graph).map_err(runtime),
                        || {
                            transaction.quads_for_pattern(
                                pattern.subject.as_ref().map(|v| v.as_ref()),
                                pattern.predicate.as_ref().map(|v| v.as_ref()),
                                pattern.object.as_ref().map(|v| v.as_ref()),
                                pattern.graph.as_ref().map(|v| v.as_ref()),
                            )
                        },
                    )
                    .map(Reply::Count)
            }
            Self::LoadGraph {
                source,
                format,
                base,
                graph,
                allow_named_graphs,
            } => {
                let mut parser = RdfParser::from_format(format)
                    .rename_blank_nodes()
                    .with_default_graph(pattern::graph(&graph)?);
                if !allow_named_graphs {
                    parser = parser.without_named_graphs();
                }
                if let Some(base) = base {
                    parser = parser.with_base_iri(base).map_err(invalid)?;
                }
                let mut reader = parser.for_slice(source.as_bytes());
                for quad in reader.by_ref() {
                    transaction.insert(&quad.map_err(invalid)?);
                }
                let prefixes = reader.prefixes().collect::<Vec<_>>();
                serde_json::to_string(&prefixes)
                    .map(Reply::Text)
                    .map_err(runtime)
            }
            Self::Graphs => {
                let graphs = transaction
                    .named_graphs()
                    .map(|graph| graph.map(|g| g.to_string()))
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(runtime)?;
                serde_json::to_string(&graphs)
                    .map(Reply::Text)
                    .map_err(runtime)
            }
            Self::ContainsGraph(source) => transaction
                .contains_named_graph(&typed::subject(source)?)
                .map(Reply::Bool)
                .map_err(runtime),
            Self::GraphLen(source) => transaction
                .graph_len(typed::graph(source)?.as_ref())
                .map(Reply::Count)
                .map_err(runtime),
            Self::Graph(source, remove) => {
                let graph = named_graph(&source)?;
                if remove {
                    transaction.remove_named_graph(&graph).map_err(runtime)?;
                } else {
                    transaction.insert_named_graph(&graph);
                }
                Ok(Reply::Done)
            }
            Self::InsertTerms(input) => {
                transaction.insert(&input.quad()?);
                Ok(Reply::Done)
            }
            Self::Remove(source) => {
                let pattern = Pattern::parse(&source)?;
                let quads = transaction
                    .quads_for_pattern(
                        pattern.subject.as_ref().map(|v| v.as_ref()),
                        pattern.predicate.as_ref().map(|v| v.as_ref()),
                        pattern.object.as_ref().map(|v| v.as_ref()),
                        pattern.graph.as_ref().map(|v| v.as_ref()),
                    )
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(runtime)?;
                for quad in quads {
                    transaction.remove(&quad);
                }
                Ok(Reply::Done)
            }
            Self::Match(source) => {
                let pattern = Pattern::parse(&source)?;
                pattern::output(transaction.quads_for_pattern(
                    pattern.subject.as_ref().map(|v| v.as_ref()),
                    pattern.predicate.as_ref().map(|v| v.as_ref()),
                    pattern.object.as_ref().map(|v| v.as_ref()),
                    pattern.graph.as_ref().map(|v| v.as_ref()),
                ))
                .map(Reply::Text)
            }
            Self::QueryGraph(source, graph) => {
                let (mut query, kind) = prepare_query(&source, false)?;
                if query.dataset().is_default_dataset() {
                    query
                        .dataset_mut()
                        .set_default_graph(vec![pattern::graph(&graph)?]);
                }
                let results = query.on_transaction(transaction).execute()?;
                let (kind, output) = query_output(results, kind)?;
                Ok(Reply::Query(kind, output))
            }
            Self::Len => transaction.len().map(Reply::Count).map_err(runtime),
            Self::Load {
                source,
                format,
                base,
            } => {
                let mut parser = RdfParser::from_format(format).rename_blank_nodes();
                if let Some(base) = base {
                    parser = parser.with_base_iri(base).map_err(invalid)?;
                }
                for quad in parser.for_slice(source.as_bytes()) {
                    transaction.insert(&quad.map_err(invalid)?);
                }
                Ok(Reply::Done)
            }
            Self::Infer(mode, destination, options) => {
                if !matches!(mode.as_str(), "rdfs" | "owlrl" | "both") {
                    return Err(Error::Invalid(format!("Unknown inference mode: {mode}")));
                }
                let destination = destination
                    .map(oxigraph::model::NamedNode::new)
                    .transpose()
                    .map_err(invalid)?
                    .map_or(oxigraph::model::GraphName::DefaultGraph, Into::into);
                crate::inference::expand_transaction(transaction, &mode, destination, options)?;
                Ok(Reply::Done)
            }
            Self::Update(source) => {
                SparqlEvaluator::new()
                    .parse_update(&source)
                    .map_err(invalid)?
                    .on_transaction(transaction)
                    .execute()
                    .map_err(runtime)?;
                Ok(Reply::Done)
            }
            Self::Query(source, union) => {
                let (query, kind) = prepare_query(&source, union)?;
                let results = query.on_transaction(transaction).execute()?;
                let (kind, output) = query_output(results, kind)?;
                Ok(Reply::Query(kind, output))
            }
            Self::Dump(format) => dump(transaction.iter(), format).map(Reply::Text),
        }
    }
}

pub(super) fn named_graph(source: &str) -> Result<oxigraph::model::NamedOrBlankNode> {
    match pattern::graph(source)? {
        oxigraph::model::GraphName::NamedNode(node) => Ok(node.into()),
        oxigraph::model::GraphName::BlankNode(node) => Ok(node.into()),
        oxigraph::model::GraphName::DefaultGraph => {
            Err(Error::Invalid("Named graph required".into()))
        }
    }
}

pub(super) fn invalid(error: impl std::fmt::Display) -> Error {
    Error::Invalid(error.to_string())
}

pub(super) fn runtime(error: impl std::fmt::Display) -> Error {
    Error::Runtime(error.to_string())
}
