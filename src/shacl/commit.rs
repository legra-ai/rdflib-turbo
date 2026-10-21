//! Apply warning policy outside dataset locks, then commit against the same revision.

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

use super::Output;
use crate::NativeDataset;
use oxigraph::model::Dataset;

pub(super) struct Pending {
    additions: Option<Dataset>,
    expected_revision: u64,
}

impl Pending {
    pub(super) fn new(additions: Dataset, expected_revision: u64) -> Self {
        Self {
            additions: Some(additions),
            expected_revision,
        }
    }

    pub(super) fn readonly(expected_revision: u64) -> Self {
        Self {
            additions: None,
            expected_revision,
        }
    }

    pub(super) fn apply(self, py: Python<'_>, dataset: &NativeDataset) -> PyResult<()> {
        py.detach(|| {
            if self.additions.is_none() {
                let current = dataset
                    .inner
                    .read()
                    .map_err(|_| PyRuntimeError::new_err("Dataset lock poisoned"))?;
                return self.check_revision(current.revision);
            }
            let mut current = dataset
                .inner
                .write()
                .map_err(|_| PyRuntimeError::new_err("Dataset lock poisoned"))?;
            self.check_revision(current.revision)?;
            if let Some(additions) = self.additions {
                let revision = current.next_revision().map_err(crate::error)?;
                current.extend(additions.iter());
                current.revision = revision;
            }
            Ok(())
        })
    }

    fn check_revision(&self, actual: u64) -> PyResult<()> {
        if actual != self.expected_revision {
            return Err(PyRuntimeError::new_err(
                "Dataset changed while processing validation warnings or report output",
            ));
        }
        Ok(())
    }
}

pub(super) struct Prepared<T> {
    output: T,
    warnings: Vec<String>,
    pending: Option<Pending>,
}

impl<T> Prepared<T> {
    pub(super) fn ready(output: T, warnings: Vec<String>) -> Self {
        Self {
            output,
            warnings,
            pending: None,
        }
    }

    pub(super) fn staged(
        output: T,
        additions: Dataset,
        expected_revision: u64,
        warnings: Vec<String>,
    ) -> Self {
        Self {
            output,
            warnings,
            pending: Some(Pending::new(additions, expected_revision)),
        }
    }

    pub(super) fn finish(
        self,
        py: Python<'_>,
        dataset: &NativeDataset,
        warning_stacklevel: usize,
    ) -> PyResult<T> {
        self.emit_warnings(py, warning_stacklevel)?;
        if let Some(pending) = self.pending {
            pending.apply(py, dataset)?;
        }
        Ok(self.output)
    }

    fn emit_warnings(&self, py: Python<'_>, stacklevel: usize) -> PyResult<()> {
        if self.warnings.is_empty() {
            return Ok(());
        }
        let facade = PyModule::import(py, "pyshacl_turbo")?;
        let category = facade.getattr("ShapeRecursionWarning")?;
        let warn = PyModule::import(py, "warnings")?.getattr("warn")?;
        for message in &self.warnings {
            warn.call1((message, &category, stacklevel))?;
        }
        Ok(())
    }
}

impl Prepared<Output> {
    pub(super) fn report(mut output: Output) -> Self {
        // Published consumers still process this field; do not emit twice.
        let warnings = std::mem::take(&mut output.3);
        Self::ready(output, warnings)
    }
}
