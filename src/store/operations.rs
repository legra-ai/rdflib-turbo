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
    ValidateInferred {
        shapes: Arc<CompiledShapes>,
        options: Options,
        inplace: bool,
        graph: Option<oxigraph::model::GraphName>,
        inference: Box<crate::shacl::InferenceStage>,
    },
    Validate {
        shapes: Arc<CompiledShapes>,
        options: Options,
        inplace: bool,
        graph: Option<oxigraph::model::GraphName>,
    },
    InsertTerms(Box<QuadInput>),
    Remove(String),
    Match(String),
    Count(String),
    QueryGraph(String, String, Option<String>),
    QueryPrepared(String, Option<String>, bool, Option<String>),
    LoadGraph {
        source: String,
        format: RdfFormat,
        base: Option<String>,
        graph: String,
        allow_named_graphs: bool,
    },
    Graphs,
    ForkGraph(Option<String>),
    ContainsGraph(TermTuple),
    GraphLen(Option<TermTuple>),
    Graph(String, bool),
    Len,
    Load {
        source: String,
        format: RdfFormat,
        base: Option<String>,
    },
    Update(String, Option<String>),
    UpdateGraph(String, String, Option<String>),
    Infer(String, Option<String>, crate::inference::InferenceRequest),
    InferGraph {
        mode: String,
        source: Option<String>,
        destination: String,
        request: crate::inference::InferenceRequest,
    },
    Query(String, bool, Option<String>),
    Dump(RdfFormat),
    DumpGraphNt(Option<TermTuple>),
}

pub(super) enum Reply {
    Validation(Box<crate::shacl::Report>),
    StagedValidation(Box<(crate::shacl::Report, super::shacl::ValidationDelta)>),
    PreparedValidation(Box<(crate::shacl::Report, u64)>),
    Done,
    Store(oxigraph::store::Store),
    Bool(bool),
    Count(usize),
    Text(String),
    GraphNt(super::output::GraphNtOutput),
    Query(String, String),
    Inference(crate::inference::InferenceOutcome),
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
            | Self::Update(..)
            | Self::UpdateGraph(..)
            | Self::Infer(..)
            | Self::InferGraph { .. } => true,
            Self::Validate { .. }
            | Self::ValidateInferred { .. }
            | Self::Match(_)
            | Self::Count(_)
            | Self::QueryGraph(..)
            | Self::Graphs
            | Self::ForkGraph(_)
            | Self::ContainsGraph(_)
            | Self::GraphLen(_)
            | Self::Len
            | Self::Query(..)
            | Self::QueryPrepared(..)
            | Self::Dump(_)
            | Self::DumpGraphNt(_) => false,
        }
    }

    pub fn execute(self, transaction: &mut Transaction<'_>) -> Result<Reply> {
        match self {
            Self::ValidateInferred {
                shapes,
                options,
                inplace,
                graph,
                inference,
            } => super::shacl_inference::validate(
                &*transaction,
                &shapes,
                options,
                inplace,
                graph,
                *inference,
            )
            .map(|output| Reply::StagedValidation(Box::new(output))),
            Self::Validate {
                shapes,
                options,
                inplace,
                graph,
            } => {
                let outcome = if let Some(graph) = &graph {
                    shapes.validate_delta(
                        &pyshacl_turbo_engine::ScopedGraph::new(&*transaction, graph.clone()),
                        options,
                    )?
                } else {
                    shapes.validate_delta(&*transaction, options)?
                };
                let report = crate::shacl::Report::try_new(outcome.report)?;
                let additions = if inplace {
                    outcome.additions
                } else {
                    Dataset::new()
                };
                Ok(Reply::StagedValidation(Box::new((
                    report,
                    super::shacl::ValidationDelta {
                        data: additions,
                        inference: Dataset::new(),
                        graph,
                    },
                ))))
            }
            Self::Count(source) => {
                let Some(pattern) = Pattern::parse_read(&source)? else {
                    return Ok(Reply::Count(0));
                };
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
                let mut parser =
                    RdfParser::from_format(format).with_default_graph(pattern::graph(&graph)?);
                // RDFLib's JSON-LD parser retains supplied blank IDs. Other document
                // syntaxes scope their blank labels to each parse operation.
                if !matches!(format, RdfFormat::JsonLd { .. }) {
                    parser = parser.rename_blank_nodes();
                }
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
            Self::ForkGraph(graph) => super::fork::fork(transaction, graph).map(Reply::Store),
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
                let Some(pattern) = Pattern::parse_read(&source)? else {
                    return Ok(Reply::Text("[]".into()));
                };
                pattern::output(transaction.quads_for_pattern(
                    pattern.subject.as_ref().map(|v| v.as_ref()),
                    pattern.predicate.as_ref().map(|v| v.as_ref()),
                    pattern.object.as_ref().map(|v| v.as_ref()),
                    pattern.graph.as_ref().map(|v| v.as_ref()),
                ))
                .map(Reply::Text)
            }
            Self::QueryPrepared(source, graph, union, bindings) => {
                let bindings = super::bindings::InitialBindings::parse(bindings.as_deref())?;
                let (mut query, kind) = super::prepared::prepare(&source, union, bindings.names())?;
                if let Some(graph) = graph
                    && query.dataset().is_default_dataset()
                {
                    query
                        .dataset_mut()
                        .set_default_graph(vec![pattern::graph(&graph)?]);
                }
                let query = bindings.apply(query)?;
                let (kind, output) =
                    query_output(query.on_transaction(transaction).execute()?, kind)?;
                Ok(Reply::Query(kind, output))
            }
            Self::QueryGraph(source, graph, bindings) => {
                let (mut query, kind) = prepare_query(&source, false, bindings.is_some())?;
                if query.dataset().is_default_dataset() {
                    query
                        .dataset_mut()
                        .set_default_graph(vec![pattern::graph(&graph)?]);
                }
                let query = super::bindings::apply(query, bindings.as_deref())?;
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
            Self::InferGraph {
                mode,
                source,
                destination,
                request,
            } => {
                if !matches!(mode.as_str(), "rdfs" | "owlrl" | "both") {
                    return Err(Error::Invalid(format!("Unknown inference mode: {mode}")));
                }
                crate::inference::scoped::expand(
                    transaction,
                    &mode,
                    source.as_deref().map(pattern::graph).transpose()?,
                    pattern::graph(&destination)?,
                    request,
                )
                .map(Reply::Inference)
            }
            Self::Infer(mode, destination, request) => {
                if !matches!(mode.as_str(), "rdfs" | "owlrl" | "both") {
                    return Err(Error::Invalid(format!("Unknown inference mode: {mode}")));
                }
                let destination = destination
                    .map(oxigraph::model::NamedNode::new)
                    .transpose()
                    .map_err(invalid)?
                    .map_or(oxigraph::model::GraphName::DefaultGraph, Into::into);
                crate::inference::expand_transaction(transaction, &mode, destination, request)
                    .map(Reply::Inference)
            }
            Self::Update(source, bindings) => {
                execute_update(transaction, source, bindings, None)?;
                Ok(Reply::Done)
            }
            Self::UpdateGraph(source, graph, bindings) => {
                execute_update(transaction, source, bindings, Some(pattern::graph(&graph)?))?;
                Ok(Reply::Done)
            }
            Self::Query(source, union, bindings) => {
                let (query, kind) = prepare_query(&source, union, bindings.is_some())?;
                let query = super::bindings::apply(query, bindings.as_deref())?;
                let results = query.on_transaction(transaction).execute()?;
                let (kind, output) = query_output(results, kind)?;
                Ok(Reply::Query(kind, output))
            }
            Self::Dump(format) => dump(transaction.iter(), format).map(Reply::Text),
            Self::DumpGraphNt(graph) => {
                let graph = typed::graph(graph)?;
                super::output::dump_graph_nt(transaction.quads_for_pattern(
                    None,
                    None,
                    None,
                    Some(graph.as_ref()),
                ))
                .map(Reply::GraphNt)
            }
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

fn execute_update(
    transaction: &mut Transaction<'_>,
    source: String,
    bindings: Option<String>,
    graph: Option<oxigraph::model::GraphName>,
) -> Result<()> {
    let evaluator = if bindings.is_some() {
        SparqlEvaluator::new().with_rdflib_query_context()
    } else {
        SparqlEvaluator::new()
    };
    let mut update = evaluator.parse_update(&source).map_err(invalid)?;
    if let Some(bindings) = bindings {
        update = update.with_initial_bindings(super::bindings::decode(&bindings)?);
    }
    if let Some(graph) = graph {
        update = update.with_default_graph(graph);
    }
    update
        .on_transaction(transaction)
        .execute()
        .map_err(runtime)
}
