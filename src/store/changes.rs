//! Compact private OWL writeback: changed identities and one owning lexical lease.
use std::sync::Mutex;

use oxigraph::store::{CommittedChanges, LexicalQuad};
use pyo3::class::gc::{PyTraverseError, PyVisit};
use pyo3::prelude::*;

use super::NativeStore;
use super::operations::{Operation, Reply};
use super::quad_stream::Stream;
use super::session::{Session, lock};
use super::term_output::{NativeTermFactories, QuadRow};
use crate::inference::{InferenceOptions, InferenceOutcome, InferenceRequest};
use crate::{Error, Result, error};

#[cfg(test)]
#[path = "prepared_changes_tests.rs"]
mod prepared_tests;

struct State {
    added: Option<Stream>,
    removed: Option<Stream>,
    closed: bool,
}

impl State {
    fn close(&mut self) {
        self.added.take();
        self.removed.take();
        self.closed = true;
    }
}

#[pyclass(frozen, module = "rdflib_turbo._native")]
pub(crate) struct NativeInferenceChanges {
    state: Mutex<State>,
    factories: Py<NativeTermFactories>,
    #[pyo3(get)]
    outcome: Py<InferenceOutcome>,
}

impl NativeInferenceChanges {
    fn new(
        py: Python<'_>,
        changes: CommittedChanges,
        outcome: InferenceOutcome,
        factories: Py<NativeTermFactories>,
    ) -> PyResult<Self> {
        let (added, removed) = changes.into_parts();
        Ok(Self {
            state: Mutex::new(State {
                added: Some(Stream::new(added)),
                removed: Some(Stream::new(removed)),
                closed: false,
            }),
            factories,
            outcome: Py::new(py, outcome)?,
        })
    }

    fn release(&self) -> Result<()> {
        lock(&self.state)?.close();
        Ok(())
    }

    fn native_batch(&self, added: bool) -> Result<(Vec<LexicalQuad>, bool)> {
        let mut state = lock(&self.state)?;
        if state.closed {
            return Err(Error::Runtime("Inference changes are closed".into()));
        }
        let stream = if added {
            &mut state.added
        } else {
            &mut state.removed
        };
        let result = stream
            .as_mut()
            .map_or_else(|| Ok((Vec::new(), true)), Stream::batch);
        match &result {
            Ok((_, true)) => {
                stream.take();
            }
            Err(_) => state.close(),
            _ => {}
        }
        result
    }

    fn batch(&self, py: Python<'_>, added: bool) -> PyResult<(Vec<QuadRow>, bool)> {
        let (rows, finished) = py.detach(|| self.native_batch(added)).map_err(error)?;
        // The native scan released its mutex before any user factory can run.
        match self.factories.bind(py).get().quad_rows(py, rows) {
            Ok(rows) => Ok((rows, finished)),
            Err(cause) => {
                // Preserve the original Python error while dropping both pure-native leases.
                let _ = py.detach(|| self.release());
                Err(cause)
            }
        }
    }
}

#[pymethods]
impl NativeInferenceChanges {
    fn __traverse__(&self, visit: PyVisit<'_>) -> std::result::Result<(), PyTraverseError> {
        visit.call(&self.factories)?;
        visit.call(&self.outcome)
    }

    fn next_additions(&self, py: Python<'_>) -> PyResult<(Vec<QuadRow>, bool)> {
        self.batch(py, true)
    }
    fn next_removals(&self, py: Python<'_>) -> PyResult<(Vec<QuadRow>, bool)> {
        self.batch(py, false)
    }
    fn close(&self, py: Python<'_>) -> PyResult<()> {
        py.detach(|| self.release()).map_err(error)
    }
}

pub(super) fn infer(
    py: Python<'_>,
    owner: &NativeStore,
    mode: String,
    graphs: (Option<String>, String),
    factories: Py<NativeTermFactories>,
    options: InferenceOptions,
    prior_error_messages: Option<Vec<String>>,
) -> PyResult<NativeInferenceChanges> {
    let (request, publisher) = InferenceRequest::capture(py, prior_error_messages, options)?;
    let session = py
        .detach(|| Session::start(owner.control.clone()))
        .map_err(error)?;
    let (source, destination) = graphs;
    let result = py.detach(|| {
        session.execute(Operation::BeginChanges)?;
        match session.execute(Operation::InferGraph {
            mode,
            source,
            destination,
            request,
        })? {
            Reply::Inference(outcome) => Ok(outcome),
            _ => unreachable!("native captured inference reply type"),
        }
    });
    // The request's producer has replied and is parked. Publish diagnostics
    // before permitting its transaction to commit; errors disconnect and abort.
    let outcome = publisher.complete(py, result)?;
    let changes = py.detach(|| session.finish_changes()).map_err(error)?;
    NativeInferenceChanges::new(py, changes, outcome, factories)
}
