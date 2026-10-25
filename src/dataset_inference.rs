//! Dataset-owned inference result construction without copying original quads.

use std::sync::RwLock;

use oxigraph::model::NamedNode;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::{DatasetState, Error, NativeDataset, inference};

impl NativeDataset {
    pub(crate) fn expand_delta(
        &self,
        py: Python<'_>,
        mode: &str,
        destination: Option<&str>,
        options: inference::InferenceOptions,
        prior_error_messages: Option<Vec<String>>,
    ) -> PyResult<(Self, inference::InferenceOutcome)> {
        if !matches!(mode, "rdfs" | "owlrl" | "both") {
            return Err(PyValueError::new_err(format!(
                "Unknown inference mode: {mode}"
            )));
        }
        let destination = destination
            .map(NamedNode::new)
            .transpose()
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        let (request, publisher) =
            inference::InferenceRequest::capture(py, prior_error_messages, options)?;
        let result = py.detach(|| {
            let mut current = self
                .inner
                .write()
                .map_err(|_| Error::Runtime("Dataset lock poisoned".into()))?;
            let revision = current.next_revision()?;
            // Namespace bindings are metadata, not a snapshot of RDF quads.
            let (data, outcome) =
                inference::expand_delta(&mut current, mode, destination.map(Into::into), request)?;
            outcome.bind_namespace(&mut current.namespaces);
            let namespaces = current.namespaces.clone();
            current.revision = revision;
            Ok((
                Self {
                    inner: RwLock::new(DatasetState::new(data, namespaces)),
                },
                outcome,
            ))
        });
        publisher.complete(py, result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::exceptions::PyRuntimeError;

    #[test]
    fn poisoned_dataset_rejects_delta_inference() {
        Python::initialize();
        let native = NativeDataset::new();
        assert!(
            std::panic::catch_unwind(|| {
                let _guard = native.inner.write().unwrap();
                panic!("simulate an interrupted native mutation");
            })
            .is_err()
        );
        Python::attach(|py| {
            let failure = native
                .infer_delta(py, "rdfs", None, false, false, None)
                .err()
                .unwrap();
            assert!(failure.is_instance_of::<PyRuntimeError>(py));
            assert!(
                failure.to_string().contains("Dataset lock poisoned"),
                "Unexpected failure: {failure}"
            );
        });
    }
}
