//! Borrow inference and validation from the same stable native view.
use oxigraph::model::{Dataset, GraphName, NamedNode};
use pyshacl_turbo_engine::{CompiledShapes, GraphRead, Options, ScopedGraph};

use super::shacl::ValidationDelta;
use crate::Result;
use crate::inference::BorrowedInference;
use crate::shacl::{InferenceStage, Report};

pub(super) fn validate<G: GraphRead + ?Sized>(
    source: &G,
    shapes: &CompiledShapes,
    options: Options,
    inplace: bool,
    graph: Option<GraphName>,
    inference: InferenceStage,
) -> Result<(Report, ValidationDelta)> {
    let (report, rules, inferred) = if let Some(graph) = &graph {
        let scoped = ScopedGraph::new(source, graph.clone());
        stage(&scoped, shapes, options, GraphName::DefaultGraph, inference)?
    } else {
        stage(
            source,
            shapes,
            options,
            NamedNode::new_unchecked("urn:pyshacl:inference").into(),
            inference,
        )?
    };
    Ok((
        report,
        ValidationDelta {
            data: if inplace { rules } else { Dataset::new() },
            inference: if inplace { inferred } else { Dataset::new() },
            graph,
        },
    ))
}

fn stage<G: GraphRead + ?Sized>(
    source: &G,
    shapes: &CompiledShapes,
    mut options: Options,
    destination: GraphName,
    inference: InferenceStage,
) -> Result<(Report, Dataset, Dataset)> {
    let mut source = BorrowedInference::new(source, destination);
    let outcome = source.expand(inference.mode.name(), inference.request)?;
    outcome.bind_namespace(&mut options.namespaces);
    let output = shapes.validate_delta(&source, options)?;
    let report =
        Report::try_new(output.report)?.with_diagnostics(!outcome.error_messages.is_empty());
    Ok((report, output.additions, source.into_additions()))
}

#[cfg(test)]
#[path = "shacl_inference_tests.rs"]
mod tests;
