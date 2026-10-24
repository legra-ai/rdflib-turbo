//! Pending changed rows remain on their original transaction worker until acceptance.
use super::operations::runtime;
use super::quad_stream::quad_bytes;
use crate::{Error, Result};
use oxigraph::store::{LexicalQuad, PreparedChanges, Transaction};

struct Part {
    pending: Option<LexicalQuad>,
    done: bool,
}

pub(super) struct StagedRows {
    // Drop pending lexical buffers before changed identities; the worker drops
    // this complete state before rolling its transaction dictionary back.
    added: Part,
    removed: Part,
    changes: PreparedChanges,
}

impl StagedRows {
    pub(super) fn prepare(transaction: &mut Transaction<'_>) -> Result<Self> {
        Ok(Self {
            changes: transaction.prepare_memory_changes().map_err(runtime)?,
            added: Part {
                pending: None,
                done: false,
            },
            removed: Part {
                pending: None,
                done: false,
            },
        })
    }

    pub(super) fn batch(
        &mut self,
        transaction: &Transaction<'_>,
        added: bool,
    ) -> Result<(Vec<LexicalQuad>, bool)> {
        let part = if added {
            &mut self.added
        } else {
            &mut self.removed
        };
        // bounded: 128 rows plus one pending row; one atomic row may exceed 64 KiB.
        let mut rows = Vec::new();
        let mut bytes = 0;
        if let Some(row) = part.pending.take() {
            bytes = quad_bytes(&row);
            rows.push(row);
        }
        if !part.done && bytes < 64 * 1024 {
            part.done = transaction
                .visit_prepared_changes(&mut self.changes, added, |row| {
                    let size = quad_bytes(&row);
                    if !rows.is_empty() && size > (64 * 1024_usize).saturating_sub(bytes) {
                        part.pending = Some(row);
                        return false;
                    }
                    bytes += size;
                    if rows.len() == rows.capacity() {
                        let capacity = if rows.is_empty() {
                            16
                        } else {
                            (rows.capacity() * 2).min(128)
                        };
                        rows.reserve_exact(capacity - rows.len());
                    }
                    rows.push(row);
                    rows.len() < 128 && bytes < 64 * 1024
                })
                .map_err(runtime)?;
        }
        Ok((rows, part.done && part.pending.is_none()))
    }

    pub(super) fn discard_removals(&mut self) {
        self.removed.pending.take();
        self.changes.discard_removals();
        self.removed.done = true;
    }

    pub(super) fn accept(self, transaction: Transaction<'_>) -> Result<()> {
        if !self.added.done
            || !self.removed.done
            || self.added.pending.is_some()
            || self.removed.pending.is_some()
        {
            drop(self);
            drop(transaction);
            return Err(Error::Runtime(
                "Prepared changes must be consumed before acceptance".into(),
            ));
        }
        transaction
            .commit_prepared_changes(self.changes)
            .map_err(runtime)
    }
}

use super::operations::{Operation, Reply};
use super::term_output::{NativeTermFactories, QuadRow};
use super::{
    NativeStore,
    session::{Session, lock},
};
use crate::{
    error,
    inference::{InferenceOptions, InferenceOutcome, InferenceRequest},
};
use pyo3::class::gc::{PyTraverseError, PyVisit};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use std::sync::{Arc, Mutex};

struct HandleState {
    session: Option<Arc<Session>>,
    in_flight: bool,
}

#[pyclass(frozen, module = "rdflib_turbo._native")]
pub(crate) struct NativePreparedInferenceChanges {
    state: Mutex<HandleState>,
    factories: Py<NativeTermFactories>,
    #[pyo3(get)]
    outcome: Py<InferenceOutcome>,
}

impl NativePreparedInferenceChanges {
    fn release(&self, py: Python<'_>) -> PyResult<()> {
        let session = lock(&self.state).map_err(error)?.session.take();
        if let Some(session) = session {
            py.detach(|| session.finish(false)).map_err(error)?;
        }
        Ok(())
    }

    fn batch(&self, py: Python<'_>, added: bool) -> PyResult<(Vec<QuadRow>, bool)> {
        let session = {
            let mut state = lock(&self.state).map_err(error)?;
            if state.in_flight {
                return Err(PyRuntimeError::new_err(
                    "Prepared restoration is in progress",
                ));
            }
            let session = state
                .session
                .as_ref()
                .ok_or_else(|| PyRuntimeError::new_err("Prepared inference changes are closed"))?
                .clone();
            state.in_flight = true;
            session
        };
        let restored = py
            .detach(|| session.read_changes(added))
            .map_err(error)
            .and_then(|(rows, done)| {
                self.factories
                    .bind(py)
                    .get()
                    .quad_rows(py, rows)
                    .map(|rows| (rows, done))
            });
        let closed = {
            let mut state = lock(&self.state).map_err(error)?;
            state.in_flight = false;
            state.session.is_none()
        };
        match restored {
            Err(cause) => {
                let _ = self.release(py);
                Err(cause)
            }
            Ok(_) if closed => Err(PyRuntimeError::new_err(
                "Prepared inference changes are closed",
            )),
            Ok(batch) => Ok(batch),
        }
    }
}

#[pymethods]
impl NativePreparedInferenceChanges {
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
        self.release(py)
    }
    fn discard_removals(&self, py: Python<'_>) -> PyResult<()> {
        let session = {
            let state = lock(&self.state).map_err(error)?;
            if state.in_flight {
                return Err(PyRuntimeError::new_err(
                    "Cannot discard while restoring prepared changes",
                ));
            }
            state
                .session
                .as_ref()
                .ok_or_else(|| PyRuntimeError::new_err("Prepared inference changes are closed"))?
                .clone()
        };
        match py.detach(|| session.discard_removals()).map_err(error) {
            Ok(()) => Ok(()),
            Err(cause) => {
                let _ = self.release(py);
                Err(cause)
            }
        }
    }

    fn accept(&self, py: Python<'_>) -> PyResult<()> {
        let session = {
            let mut state = lock(&self.state).map_err(error)?;
            if state.in_flight {
                return Err(PyRuntimeError::new_err(
                    "Cannot accept while restoring prepared changes",
                ));
            }
            state
                .session
                .take()
                .ok_or_else(|| PyRuntimeError::new_err("Prepared inference changes are closed"))?
        };
        py.detach(|| session.accept_changes()).map_err(error)
    }
}

pub(super) fn prepare(
    py: Python<'_>,
    owner: &NativeStore,
    mode: String,
    graphs: (Option<String>, String),
    factories: Py<NativeTermFactories>,
    options: InferenceOptions,
    prior_error_messages: Option<Vec<String>>,
) -> PyResult<NativePreparedInferenceChanges> {
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
            _ => unreachable!("native prepared inference outcome"),
        }
    });
    let outcome = publisher.complete(py, result)?;
    py.detach(|| session.prepare_changes()).map_err(error)?;
    let outcome = Py::new(py, outcome)?;
    Ok(NativePreparedInferenceChanges {
        state: Mutex::new(HandleState {
            session: Some(Arc::new(session)),
            in_flight: false,
        }),
        factories,
        outcome,
    })
}
