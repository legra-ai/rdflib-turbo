//! Materialize RDFLib objects with the GIL after releasing the native scan lock.
use oxigraph::store::{LexicalTerm, TermText};
use pyo3::class::gc::{PyTraverseError, PyVisit};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyString, PyType};

use super::read;
use crate::error;

type Row = (Option<Py<PyAny>>, Option<Py<PyAny>>, Option<Py<PyAny>>);
pub(super) type FirstBatch = (Vec<Row>, Option<NativeReadCursor>);

#[pyclass(frozen, module = "rdflib_turbo._native")]
pub(crate) struct NativeTermFactories {
    uri: Py<PyAny>,
    bnode: Py<PyAny>,
    literal: Py<PyAny>,
    allocate: Py<PyAny>,
}

#[pymethods]
impl NativeTermFactories {
    #[new]
    pub(super) fn new(
        py: Python<'_>,
        uri: Py<PyAny>,
        bnode: Py<PyAny>,
        literal: Py<PyAny>,
        allocate: Py<PyAny>,
    ) -> PyResult<Self> {
        for class in [&uri, &bnode] {
            let class = class.bind(py).cast::<PyType>()?;
            if !class.is_subclass_of::<PyString>()? {
                return Err(PyTypeError::new_err(
                    "Identifier factories must be string subclasses",
                ));
            }
        }
        for callable in [&literal, &allocate] {
            if !callable.bind(py).is_callable() {
                return Err(PyTypeError::new_err(
                    "Term restoration factories must be callable",
                ));
            }
        }
        Ok(Self {
            uri,
            bnode,
            literal,
            allocate,
        })
    }

    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.uri)?;
        visit.call(&self.bnode)?;
        visit.call(&self.literal)?;
        visit.call(&self.allocate)
    }
}

impl NativeTermFactories {
    fn term(&self, py: Python<'_>, source: Option<LexicalTerm>) -> PyResult<Option<Py<PyAny>>> {
        let Some(source) = source else {
            return Ok(None);
        };
        let term = match source {
            LexicalTerm::NamedNode(value) => self
                .allocate
                .bind(py)
                .call1((self.uri.bind(py), value.as_str()))?,
            LexicalTerm::BlankNode(value) => self
                .allocate
                .bind(py)
                .call1((self.bnode.bind(py), value.as_str()))?,
            LexicalTerm::Literal {
                value,
                datatype,
                language,
            } => self.literal.bind(py).call1((
                value.as_str(),
                datatype.as_ref().map(TermText::as_str),
                language.as_ref().map(TermText::as_str),
            ))?,
        };
        Ok(Some(term.unbind()))
    }

    fn rows(&self, py: Python<'_>, rows: Vec<read::Row>) -> PyResult<Vec<Row>> {
        // bounded: the native reader caps every input batch at 128 rows / ~64 KiB.
        rows.into_iter()
            .map(|(s, p, o)| Ok((self.term(py, s)?, self.term(py, p)?, self.term(py, o)?)))
            .collect()
    }
}

pub(super) fn first(
    py: Python<'_>,
    (rows, cursor): read::FirstBatch,
    factories: Py<NativeTermFactories>,
) -> PyResult<FirstBatch> {
    // The frozen descriptor is borrowed only after the native scan released its lock.
    match factories.bind(py).get().rows(py, rows) {
        Ok(rows) => Ok((
            rows,
            cursor.map(|cursor| NativeReadCursor { cursor, factories }),
        )),
        Err(cause) => {
            // Drop the pure native continuation before any descriptor reference can be dropped.
            drop(cursor);
            Err(cause)
        }
    }
}

#[pyclass(frozen, module = "rdflib_turbo._native")]
pub(crate) struct NativeReadCursor {
    cursor: read::ReadCursor,
    factories: Py<NativeTermFactories>,
}

#[pymethods]
impl NativeReadCursor {
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        // One actual Python reference is one GC edge, even when descriptors are shared.
        visit.call(&self.factories)
    }

    fn next_batch(&self, py: Python<'_>) -> PyResult<(Vec<Row>, bool)> {
        let (rows, finished) = py.detach(|| self.cursor.batch()).map_err(error)?;
        match self.factories.bind(py).get().rows(py, rows) {
            Ok(rows) => Ok((rows, finished)),
            Err(cause) => {
                // Preserve the original Python error; release the snapshot without Python callbacks.
                let _ = py.detach(|| self.cursor.release());
                Err(cause)
            }
        }
    }

    fn close(&self, py: Python<'_>) -> PyResult<()> {
        py.detach(|| self.cursor.release()).map_err(error)
    }
}
