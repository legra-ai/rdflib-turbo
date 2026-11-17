//! Python-bound inference operations of [`NativeStore`].

use pyo3::prelude::*;

use super::native_store::NativeStore;
use super::operations::{self, Operation};
use super::session::Session;
use super::{changes, prepared_changes, term_output};

impl NativeStore {
    pub(super) fn prepare_inference_changes_for(
        &self,
        py: Python<'_>,
        mode: String,
        graphs: (Option<String>, String),
        factories: Py<term_output::NativeTermFactories>,
        options: (bool, bool),
        prior_error_messages: Option<Vec<String>>,
    ) -> PyResult<prepared_changes::NativePreparedInferenceChanges> {
        prepared_changes::prepare(
            py,
            self,
            mode,
            graphs,
            factories,
            crate::inference::InferenceOptions {
                axiomatic_triples: options.0,
                datatype_axioms: options.1,
            },
            prior_error_messages,
        )
    }

    pub(super) fn infer_changes_for(
        &self,
        py: Python<'_>,
        mode: String,
        graphs: (Option<String>, String),
        factories: Py<term_output::NativeTermFactories>,
        options: (bool, bool),
        prior_error_messages: Option<Vec<String>>,
    ) -> PyResult<changes::NativeInferenceChanges> {
        changes::infer(
            py,
            self,
            mode,
            graphs,
            factories,
            crate::inference::InferenceOptions {
                axiomatic_triples: options.0,
                datatype_axioms: options.1,
            },
            prior_error_messages,
        )
    }

    pub(super) fn infer_inline(
        &self,
        py: Python<'_>,
        mode: String,
        destination: Option<String>,
        axiomatic_triples: bool,
        datatype_axioms: bool,
        prior_error_messages: Option<Vec<String>>,
    ) -> PyResult<crate::inference::InferenceOutcome> {
        let (request, publisher) = crate::inference::InferenceRequest::capture(
            py,
            prior_error_messages,
            crate::inference::InferenceOptions {
                axiomatic_triples,
                datatype_axioms,
            },
        )?;
        let result = py.detach(|| {
            match Session::execute_inline(
                self.control.clone(),
                Operation::Infer(mode, destination, request),
            )? {
                operations::Reply::Inference(outcome) => Ok(outcome),
                _ => unreachable!("native inference reply type"),
            }
        });
        publisher.complete(py, result)
    }

    pub(super) fn infer_graph_inline(
        &self,
        py: Python<'_>,
        mode: String,
        graphs: (Option<String>, String),
        axiomatic_triples: bool,
        datatype_axioms: bool,
        prior_error_messages: Option<Vec<String>>,
    ) -> PyResult<crate::inference::InferenceOutcome> {
        let (source_graph, destination_graph) = graphs;
        let (request, publisher) = crate::inference::InferenceRequest::capture(
            py,
            prior_error_messages,
            crate::inference::InferenceOptions {
                axiomatic_triples,
                datatype_axioms,
            },
        )?;
        let result = py.detach(|| {
            match Session::execute_inline(
                self.control.clone(),
                Operation::InferGraph {
                    mode,
                    source: source_graph,
                    destination: destination_graph,
                    request,
                },
            )? {
                operations::Reply::Inference(outcome) => Ok(outcome),
                _ => unreachable!("native scoped inference reply type"),
            }
        });
        publisher.complete(py, result)
    }
}
