//! Apply warning policy outside dataset locks, then commit against the same revision.

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

use super::Output;
use crate::NativeDataset;
use oxigraph::model::Dataset;

struct Pending {
    additions: Dataset,
    expected_revision: u64,
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
            pending: Some(Pending {
                additions,
                expected_revision,
            }),
        }
    }

    pub(super) fn finish(self, py: Python<'_>, dataset: &NativeDataset) -> PyResult<T> {
        self.emit_warnings(py)?;
        if let Some(pending) = self.pending {
            py.detach(|| {
                let mut current = dataset
                    .inner
                    .write()
                    .map_err(|_| PyRuntimeError::new_err("Dataset lock poisoned"))?;
                if current.revision != pending.expected_revision {
                    return Err(PyRuntimeError::new_err(
                        "Dataset changed while processing validation warnings",
                    ));
                }
                let revision = current.next_revision()?;
                current.extend(pending.additions.iter());
                current.revision = revision;
                Ok(())
            })?;
        }
        Ok(self.output)
    }

    fn emit_warnings(&self, py: Python<'_>) -> PyResult<()> {
        if self.warnings.is_empty() {
            return Ok(());
        }
        let facade = PyModule::import(py, "pyshacl_turbo")?;
        let category = facade.getattr("ShapeRecursionWarning")?;
        let warn = PyModule::import(py, "warnings")?.getattr("warn")?;
        for message in &self.warnings {
            // The public facade's helper adds one Python frame and uses level 3.
            // Native emission omits that helper frame, retaining its caller at 2.
            warn.call1((message, &category, 2))?;
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
