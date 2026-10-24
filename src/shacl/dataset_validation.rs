//! Keep Dataset additions private until the consumer accepts warnings and report output.
use std::sync::Mutex;

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

use super::commit::{Pending, Prepared};
use super::{Output, Settings, SharedShapes, error, output};
use crate::NativeDataset;

impl SharedShapes {
    pub(super) fn validate_dataset(
        &self,
        py: Python<'_>,
        dataset: &NativeDataset,
        settings: &str,
        inplace: bool,
    ) -> PyResult<Output> {
        if let Some(prepared) = self.infer_dataset(py, dataset, settings, inplace)? {
            return prepared.finish(py, dataset, 4);
        }
        let mut options = Settings::parse(settings)?;
        let prepared = py.detach(|| -> PyResult<Prepared<Output>> {
            if !inplace || !self.inner.has_active_rules(&options) {
                let data = dataset
                    .inner
                    .read()
                    .map_err(|_| PyRuntimeError::new_err("Dataset lock poisoned"))?;
                options.namespaces = data.namespaces.clone();
                return Ok(Prepared::report(output(
                    self.inner
                        .validate_staged(&data.data, options)
                        .map_err(error)?,
                )?));
            }
            let mut data = dataset
                .inner
                .write()
                .map_err(|_| PyRuntimeError::new_err("Dataset lock poisoned"))?;
            options.namespaces = data.namespaces.clone();
            let revision = data.next_revision().map_err(crate::error)?;
            let outcome = self
                .inner
                .validate_delta(&data.data, options)
                .map_err(error)?;
            let mut result = output(outcome.report)?;
            if result.3.is_empty() {
                data.extend(outcome.additions.iter());
                data.revision = revision;
                Ok(Prepared::report(result))
            } else {
                let warnings = std::mem::take(&mut result.3);
                Ok(Prepared::staged(
                    result,
                    outcome.additions,
                    data.revision,
                    warnings,
                ))
            }
        })?;
        // The source adapter adds a generator and contextlib frame.
        prepared.finish(py, dataset, 4)
    }

    pub(super) fn prepare_dataset_validation(
        &self,
        py: Python<'_>,
        owner: Py<NativeDataset>,
        settings: &str,
        inplace: bool,
    ) -> PyResult<PreparedDatasetValidation> {
        let dataset = owner.bind(py).get();
        if let Some((report, pending)) =
            self.prepare_inferred_dataset(py, dataset, settings, inplace)?
        {
            return Ok(PreparedDatasetValidation::new(owner, report, pending));
        }
        let mut options = Settings::parse(settings)?;
        let (report, pending) = py.detach(|| {
            let data = dataset
                .inner
                .read()
                .map_err(|_| PyRuntimeError::new_err("Dataset lock poisoned"))?;
            options.namespaces = data.namespaces.clone();
            if !inplace || !self.inner.has_active_rules(&options) {
                let report = output(
                    self.inner
                        .validate_staged(&data.data, options)
                        .map_err(error)?,
                )?;
                let pending = inplace.then(|| Pending::readonly(data.revision));
                return Ok::<_, PyErr>((report, pending));
            }
            // Reject exhausted mutation counters before executing a write-capable validation.
            data.next_revision().map_err(crate::error)?;
            let outcome = self
                .inner
                .validate_delta(&data.data, options)
                .map_err(error)?;
            Ok((
                output(outcome.report)?,
                Some(Pending::new(outcome.additions, data.revision)),
            ))
        })?;
        Ok(PreparedDatasetValidation::new(owner, report, pending))
    }
}

struct State {
    output: Option<Output>,
    pending: Option<Pending>,
    accepted: bool,
}

/// Owns a detached report and private delta; abandonment simply discards the delta.
#[pyclass(frozen, module = "rdflib_turbo._native")]
pub(super) struct PreparedDatasetValidation {
    owner: Py<NativeDataset>,
    state: Mutex<State>,
}

impl PreparedDatasetValidation {
    fn new(owner: Py<NativeDataset>, report: Output, pending: Option<Pending>) -> Self {
        Self {
            owner,
            state: Mutex::new(State {
                output: Some(report),
                pending,
                accepted: false,
            }),
        }
    }
}

#[pymethods]
impl PreparedDatasetValidation {
    pub(super) fn output(&self) -> PyResult<Output> {
        self.state
            .lock()
            .map_err(|_| PyRuntimeError::new_err("Validation state lock poisoned"))?
            .output
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Validation output was already consumed"))
    }

    pub(super) fn accept(&self, py: Python<'_>) -> PyResult<()> {
        let pending = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| PyRuntimeError::new_err("Validation state lock poisoned"))?;
            if state.output.is_some() {
                return Err(PyRuntimeError::new_err(
                    "Validation output must be consumed before acceptance",
                ));
            }
            if state.accepted {
                return Err(PyRuntimeError::new_err(
                    "Validation output was already accepted",
                ));
            }
            state.accepted = true;
            state.pending.take()
        };
        if let Some(pending) = pending {
            pending.apply(py, self.owner.bind(py).get())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULE: &str = "@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:S> sh:targetNode <urn:s>; sh:rule [a sh:TripleRule; sh:subject sh:this; sh:predicate <urn:derived>; sh:object true].";

    #[test]
    fn prepared_report_is_detached_and_acceptance_applies_once() {
        Python::initialize();
        Python::attach(|py| {
            let owner = Py::new(py, NativeDataset::new()).unwrap();
            let data = owner.bind(py).get();
            data.load(py, "<urn:s> <urn:p> <urn:o>.", "turtle", None, false)
                .unwrap();
            let program = SharedShapes::new(
                py,
                RULE,
                true,
                vec![],
                true,
                crate::native_tests::host_regex_policy(py),
            )
            .unwrap();
            let prepared = program
                .prepare_dataset_validation(py, owner.clone_ref(py), "{}", true)
                .unwrap();
            let mut report = prepared.output().unwrap();
            assert!(report.0);
            report.1.0.clear();
            report.1.1.clear();
            assert_eq!(data.__len__().unwrap(), 1);
            prepared.accept(py).unwrap();
            assert_eq!(data.__len__().unwrap(), 2);
            assert!(
                prepared
                    .accept(py)
                    .unwrap_err()
                    .to_string()
                    .contains("already accepted")
            );
            assert!(
                prepared
                    .output()
                    .unwrap_err()
                    .to_string()
                    .contains("already consumed")
            );
            assert_eq!(data.__len__().unwrap(), 2);
        });
    }

    #[test]
    fn abandoned_or_invalidly_consumed_reports_never_apply_additions() {
        Python::initialize();
        Python::attach(|py| {
            let owner = Py::new(py, NativeDataset::new()).unwrap();
            let data = owner.bind(py).get();
            data.load(
                py,
                "@prefix ex:<urn:>. ex:s ex:p ex:o.",
                "turtle",
                None,
                false,
            )
            .unwrap();
            let before = data.dump(py, "nquads").unwrap();
            let program = SharedShapes::new(
                py,
                RULE,
                true,
                vec![],
                true,
                crate::native_tests::host_regex_policy(py),
            )
            .unwrap();
            for consumed in [false, true] {
                let prepared = program
                    .prepare_dataset_validation(py, owner.clone_ref(py), "{}", true)
                    .unwrap();
                if consumed {
                    prepared.output().unwrap();
                } else {
                    assert!(prepared.accept(py).is_err());
                }
                drop(prepared);
                assert_eq!(data.dump(py, "nquads").unwrap(), before);
            }
        });
    }
}
