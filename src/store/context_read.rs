//! Exact-SPO context scans decode only graph names on one owning snapshot.
use super::term_output::NativeTermFactories;
use super::{operations::runtime, quad_read, read_input, session::lock};
use crate::{Error, Result, error};
use oxigraph::store::{GeneralizedLexicalGraphRows, LexicalGraphName, Store};
use pyo3::class::gc::{PyTraverseError, PyVisit};
use pyo3::prelude::*;
use std::sync::Mutex;

struct Stream {
    iter: Option<GeneralizedLexicalGraphRows<'static>>,
    pending: Option<LexicalGraphName>,
}
pub(super) type FirstBatch = (Vec<Option<Py<PyAny>>>, Option<NativeContextCursor>);

pub(super) fn open(
    store: &Store,
    pattern: &read_input::Pattern,
) -> Result<NativeContextCursorState> {
    let subject = quad_read::prepare(&pattern.0)?;
    let predicate = quad_read::prepare(&pattern.1)?;
    let object = quad_read::prepare(&pattern.2)?;
    let (Some(subject), Some(predicate), Some(object)) = (subject, predicate, object) else {
        return Err(Error::Invalid(
            "Context reads require a fully bound statement".into(),
        ));
    };
    let iter = store
        .read_snapshot()
        .generalized_quads_for_pattern(
            Some(subject.term_ref()),
            Some(predicate.term_ref()),
            Some(object.term_ref()),
            None,
        )
        .project_graph_lexical();
    Ok(NativeContextCursorState(Mutex::new(Stream {
        iter: Some(iter),
        pending: None,
    })))
}

pub(super) struct NativeContextCursorState(Mutex<Stream>);
impl NativeContextCursorState {
    fn release(&self) -> Result<()> {
        let mut stream = lock(&self.0)?;
        stream.iter.take();
        stream.pending.take();
        Ok(())
    }
    fn batch(&self) -> Result<(Vec<LexicalGraphName>, bool)> {
        let mut stream = lock(&self.0)?;
        let mut rows = Vec::new();
        let mut bytes = 0;
        // bounded: at most128 graph owners plus one pending; one name may exceed64KiB.
        while rows.len() < 128 {
            let graph = match stream.pending.take() {
                Some(graph) => graph,
                None => match stream.iter.as_mut().and_then(Iterator::next) {
                    Some(Ok(graph)) => graph,
                    Some(Err(cause)) => {
                        stream.iter.take();
                        return Err(runtime(cause));
                    }
                    None => {
                        stream.iter.take();
                        break;
                    }
                },
            };
            let size = match &graph {
                LexicalGraphName::DefaultGraph => 0,
                LexicalGraphName::NamedNode(text) | LexicalGraphName::BlankNode(text) => {
                    64 + text.as_str().len()
                }
            };
            if !rows.is_empty() && size > (64 * 1024_usize).saturating_sub(bytes) {
                stream.pending = Some(graph);
                break;
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
            rows.push(graph);
            if bytes >= 64 * 1024 {
                break;
            }
        }
        Ok((rows, stream.iter.is_none()))
    }
}

#[pyclass(frozen, module = "rdflib_turbo._native")]
pub(crate) struct NativeContextCursor {
    state: NativeContextCursorState,
    factories: Py<NativeTermFactories>,
}
impl NativeContextCursor {
    pub(super) fn lazy(
        iter: GeneralizedLexicalGraphRows<'static>,
        factories: Py<NativeTermFactories>,
    ) -> Self {
        Self {
            state: NativeContextCursorState(Mutex::new(Stream {
                iter: Some(iter),
                pending: None,
            })),
            factories,
        }
    }
}
pub(super) fn first(
    py: Python<'_>,
    state: NativeContextCursorState,
    factories: Py<NativeTermFactories>,
) -> PyResult<FirstBatch> {
    let cursor = NativeContextCursor { state, factories };
    let (rows, done) = cursor.next_batch(py)?;
    Ok((rows, if done { None } else { Some(cursor) }))
}
#[pymethods]
impl NativeContextCursor {
    fn __traverse__(&self, visit: PyVisit<'_>) -> std::result::Result<(), PyTraverseError> {
        visit.call(&self.factories)
    }
    fn next_batch(&self, py: Python<'_>) -> PyResult<(Vec<Option<Py<PyAny>>>, bool)> {
        let (rows, done) = py.detach(|| self.state.batch()).map_err(error)?;
        match self.factories.bind(py).get().graph_rows(py, rows) {
            Ok(rows) => Ok((rows, done)),
            Err(cause) => {
                let _ = py.detach(|| self.state.release());
                Err(cause)
            }
        }
    }
    fn close(&self, py: Python<'_>) -> PyResult<()> {
        py.detach(|| self.state.release()).map_err(error)
    }
}
