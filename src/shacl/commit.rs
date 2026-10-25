//! Apply warning policy outside dataset locks, then commit against the same revision.

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

use super::Output;
use crate::NativeDataset;
use crate::inference::InferenceDelta;
use oxigraph::model::Dataset;

pub(super) struct Pending {
    additions: Option<Dataset>,
    inference: Option<InferenceDelta>,
    diagnostics: bool,
    expected_revision: u64,
}

impl Pending {
    pub(super) fn new(additions: Dataset, expected_revision: u64) -> Self {
        Self {
            additions: Some(additions),
            inference: None,
            diagnostics: false,
            expected_revision,
        }
    }

    pub(super) fn readonly(expected_revision: u64) -> Self {
        Self {
            additions: None,
            inference: None,
            diagnostics: false,
            expected_revision,
        }
    }

    pub(super) fn inferred(
        additions: Dataset,
        inference: InferenceDelta,
        diagnostics: bool,
        expected_revision: u64,
    ) -> Self {
        Self {
            additions: Some(additions),
            inference: Some(inference),
            diagnostics,
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
                {
                    let mut transaction = current.data.transaction();
                    if let Some(inference) = self.inference {
                        inference.apply_dataset(&mut transaction);
                    }
                    for quad in additions.iter() {
                        transaction.insert(quad);
                    }
                    transaction.commit();
                }
                if self.diagnostics {
                    crate::inference::bind_error_namespace(&mut current.namespaces);
                }
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
    pub(super) fn with_pending(output: T, warnings: Vec<String>, pending: Option<Pending>) -> Self {
        Self {
            output,
            warnings,
            pending,
        }
    }
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

#[cfg(test)]
mod generalized_tests {
    use super::*;
    use oxigraph::model::*;
    use std::sync::RwLock;

    #[test]
    fn accepted_pending_commit_keeps_strict_rules_and_full_inference_rows() {
        let rule = Quad::new(
            NamedNode::new_unchecked("urn:s"),
            NamedNode::new_unchecked("urn:p"),
            NamedNode::new_unchecked("urn:o"),
            GraphName::DefaultGraph,
        );
        let inferred = GeneralizedQuad::new(
            Literal::new_typed_literal("01", NamedNode::new_unchecked("urn:datatype")),
            NamedNode::new_unchecked("urn:inferred"),
            NamedNode::new_unchecked("urn:value"),
            GraphName::DefaultGraph,
        );
        let rules = Dataset::from_iter([rule.clone()]);
        let inference = GeneralizedDataset::from_iter([inferred.clone()]);
        let owner = NativeDataset {
            inner: RwLock::new(crate::DatasetState::new(
                GeneralizedDataset::new(),
                Vec::new(),
            )),
        };
        Python::initialize();
        Python::attach(|py| {
            Pending::inferred(rules, InferenceDelta::from_additions(inference), false, 0)
                .apply(py, &owner)
                .unwrap();
            let state = owner.inner.read().unwrap();
            assert!(state.contains(rule.as_ref()));
            assert!(state.contains(inferred.as_ref()));
            assert_eq!(state.len(), 2);
            assert_eq!(state.revision, 1);
        });
    }
}
