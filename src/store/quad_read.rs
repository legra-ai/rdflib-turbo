//! Physical quad reads distinguish any/default/named scopes explicitly.
use super::term_output::{NativeTermFactories, QuadRow};
use super::typed_view::{Position, PreparedTerm};
use super::{quad_stream::Stream, read_input, session::lock};
use crate::{Error, Result, error};
use oxigraph::model::GraphNameRef;
use oxigraph::store::Store;
use pyo3::class::gc::{PyTraverseError, PyVisit};
use pyo3::prelude::*;
use std::sync::Mutex;

pub(super) type FirstBatch = (Vec<QuadRow>, Option<NativeQuadCursor>);

pub(super) fn prepare(term: &Option<read_input::TermTuple>) -> Result<Option<PreparedTerm<'_>>> {
    read_input::view(term)
        .map(|value| PreparedTerm::new(value, Position::Data))
        .transpose()
}

pub(super) fn open(
    store: &Store,
    pattern: &read_input::Pattern,
    scope: &(String, Option<read_input::TermTuple>),
) -> Result<Stream> {
    let subject = prepare(&pattern.0)?;
    let predicate = prepare(&pattern.1)?;
    let object = prepare(&pattern.2)?;
    let graph = read_input::view(&scope.1)
        .map(|value| PreparedTerm::new(value, Position::GraphName))
        .transpose()?;
    let selected = match (scope.0.as_str(), graph.as_ref()) {
        ("any", None) => None,
        ("default", None) => Some(GraphNameRef::DefaultGraph),
        ("graph", Some(graph)) => Some(graph.graph_name_ref().into()),
        _ => {
            return Err(Error::Invalid(
                "Expected scope ('any', None), ('default', None), or ('graph', graph term)".into(),
            ));
        }
    };
    Ok(Stream::new(store.generalized_quads_for_pattern(
        subject.as_ref().map(PreparedTerm::term_ref),
        predicate.as_ref().map(PreparedTerm::term_ref),
        object.as_ref().map(PreparedTerm::term_ref),
        selected,
    )))
}

#[pyclass(frozen, module = "rdflib_turbo._native")]
pub(crate) struct NativeQuadCursor {
    stream: Mutex<Option<Stream>>,
    factories: Py<NativeTermFactories>,
}

pub(super) fn first(
    py: Python<'_>,
    stream: Stream,
    factories: Py<NativeTermFactories>,
) -> PyResult<FirstBatch> {
    let cursor = NativeQuadCursor {
        stream: Mutex::new(Some(stream)),
        factories,
    };
    let (rows, done) = cursor.next_batch(py)?;
    Ok((rows, if done { None } else { Some(cursor) }))
}

impl NativeQuadCursor {
    fn release(&self) -> Result<()> {
        lock(&self.stream)?.take();
        Ok(())
    }
}

#[pymethods]
impl NativeQuadCursor {
    fn __traverse__(&self, visit: PyVisit<'_>) -> std::result::Result<(), PyTraverseError> {
        visit.call(&self.factories)
    }

    fn next_batch(&self, py: Python<'_>) -> PyResult<(Vec<QuadRow>, bool)> {
        let (rows, done) = py
            .detach(|| {
                let mut guard = lock(&self.stream)?;
                let result = guard
                    .as_mut()
                    .map_or_else(|| Ok((Vec::new(), true)), Stream::batch);
                if !matches!(&result, Ok((_, false))) {
                    guard.take();
                }
                result
            })
            .map_err(error)?;
        // Callers may close/reenter from their factories: no native lock is held.
        match self.factories.bind(py).get().quad_rows(py, rows) {
            Ok(rows) => Ok((rows, done)),
            Err(cause) => {
                let _ = py.detach(|| self.release());
                Err(cause)
            }
        }
    }

    fn close(&self, py: Python<'_>) -> PyResult<()> {
        py.detach(|| self.release()).map_err(error)
    }
}
