//! The Python read cursor that streams later batches of materialized rows.
use pyo3::class::gc::{PyTraverseError, PyVisit};
use pyo3::prelude::*;

use super::super::read;
use super::factories::{NativeTermFactories, Row};
use crate::error;

pub(in crate::store) type FirstBatch = (Vec<Row>, Option<NativeReadCursor>);

pub(in crate::store) fn first(
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
