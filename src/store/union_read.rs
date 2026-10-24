//! Bounded union statements and unconsumed exact contexts from one pinned reader.
use super::term_output::NativeTermFactories;
use super::typed_view::{Position, PreparedTerm};
use super::{operations::runtime, quad_read, read::term_bytes, read_input, session::lock};
use crate::{Error, Result, error};
use oxigraph::model::GraphNameRef;
use oxigraph::store::{GeneralizedLexicalUnionRows, LexicalUnionRow, Store};
use pyo3::class::gc::{PyTraverseError, PyVisit};
use pyo3::prelude::*;
use std::sync::Mutex;
pub(super) type Row = (
    (Py<PyAny>, Py<PyAny>, Py<PyAny>),
    super::context_read::NativeContextCursor,
);
pub(super) type FirstBatch = (Vec<Row>, Option<NativeUnionCursor>);
pub(super) struct Stream {
    iter: Option<GeneralizedLexicalUnionRows>,
    pending: Option<LexicalUnionRow>,
}
pub(super) fn open(
    store: &Store,
    pattern: &read_input::Pattern,
    scope: Option<&(String, Option<read_input::TermTuple>)>,
) -> Result<Stream> {
    let s = quad_read::prepare(&pattern.0)?;
    let p = quad_read::prepare(&pattern.1)?;
    let o = quad_read::prepare(&pattern.2)?;
    let graph = scope
        .and_then(|scope| read_input::view(&scope.1))
        .map(|value| PreparedTerm::new(value, Position::GraphName))
        .transpose()?;
    let selected = match (scope.map(|scope| scope.0.as_str()), graph.as_ref()) {
        (None | Some("any"), None) => None,
        (Some("default"), None) => Some(GraphNameRef::DefaultGraph),
        (Some("graph"), Some(graph)) => Some(graph.graph_name_ref().into()),
        _ => {
            return Err(Error::Invalid(
                "Expected scope ('any', None), ('default', None), or ('graph', graph term)".into(),
            ));
        }
    };
    let iter = store
        .read_snapshot()
        .generalized_quads_for_pattern(
            s.as_ref().map(|s| s.term_ref()),
            p.as_ref().map(|p| p.term_ref()),
            o.as_ref().map(|o| o.term_ref()),
            selected,
        )
        .project_union_lexical();
    Ok(Stream {
        iter: Some(iter),
        pending: None,
    })
}
impl Stream {
    fn batch(&mut self) -> Result<(Vec<LexicalUnionRow>, bool)> {
        let mut rows = Vec::new();
        let mut bytes = 0;
        // bounded:128 statement/context pairs plus one pending; atomic oversized row allowed.
        while rows.len() < 128 {
            let row = match self.pending.take() {
                Some(row) => row,
                None => match self.iter.as_mut().and_then(Iterator::next) {
                    Some(Ok(row)) => row,
                    Some(Err(cause)) => {
                        self.iter.take();
                        return Err(runtime(cause));
                    }
                    None => {
                        self.iter.take();
                        break;
                    }
                },
            };
            let size = row.terms.iter().map(term_bytes).sum::<usize>();
            if !rows.is_empty() && size > (64 * 1024_usize).saturating_sub(bytes) {
                self.pending = Some(row);
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
            rows.push(row);
            if bytes >= 64 * 1024 {
                break;
            }
        }
        Ok((rows, self.iter.is_none()))
    }
}
#[pyclass(frozen, module = "rdflib_turbo._native")]
pub(crate) struct NativeUnionCursor {
    stream: Mutex<Stream>,
    factories: Py<NativeTermFactories>,
}
pub(super) fn first(
    py: Python<'_>,
    stream: Stream,
    factories: Py<NativeTermFactories>,
) -> PyResult<FirstBatch> {
    let cursor = NativeUnionCursor {
        stream: Mutex::new(stream),
        factories,
    };
    let (rows, done) = cursor.next_batch(py)?;
    Ok((rows, if done { None } else { Some(cursor) }))
}
impl NativeUnionCursor {
    fn release(&self) -> Result<()> {
        let mut stream = lock(&self.stream)?;
        stream.iter.take();
        stream.pending.take();
        Ok(())
    }
}
#[pymethods]
impl NativeUnionCursor {
    fn __traverse__(&self, visit: PyVisit<'_>) -> std::result::Result<(), PyTraverseError> {
        visit.call(&self.factories)
    }
    fn next_batch(&self, py: Python<'_>) -> PyResult<(Vec<Row>, bool)> {
        let (rows, done) = py.detach(|| lock(&self.stream)?.batch()).map_err(error)?;
        match self
            .factories
            .bind(py)
            .get()
            .union_rows(py, rows, &self.factories)
        {
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
