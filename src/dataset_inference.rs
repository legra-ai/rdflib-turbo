//! Dataset-owned inference result construction without copying original quads.

use std::sync::RwLock;

use oxigraph::model::NamedNode;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;

use crate::{DatasetState, NativeDataset, error, inference};

impl NativeDataset {
    pub(crate) fn expand_delta(
        &self,
        py: Python<'_>,
        mode: &str,
        destination: Option<&str>,
        options: inference::InferenceOptions,
    ) -> PyResult<Self> {
        if !matches!(mode, "rdfs" | "owlrl" | "both") {
            return Err(PyValueError::new_err(format!(
                "Unknown inference mode: {mode}"
            )));
        }
        let destination = destination
            .map(NamedNode::new)
            .transpose()
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        py.detach(|| {
            let mut current = self
                .inner
                .write()
                .map_err(|_| PyRuntimeError::new_err("Dataset lock poisoned"))?;
            // Namespace bindings are metadata, not a snapshot of RDF quads.
            let namespaces = current.namespaces.clone();
            let data =
                inference::expand_delta(&mut current, mode, destination.map(Into::into), options)
                    .map_err(error)?;
            Ok(Self {
                inner: RwLock::new(DatasetState { data, namespaces }),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                .infer_delta(py, "rdfs", None, false, false)
                .err()
                .unwrap();
            assert!(failure.is_instance_of::<PyRuntimeError>(py));
            assert!(failure.to_string().contains("Dataset lock poisoned"));
        });
    }
}
