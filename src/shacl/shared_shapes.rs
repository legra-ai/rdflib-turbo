//! Compiled SHACL shapes shared by datasets, stores and transactions.
use std::sync::Arc;

use oxigraph::io::RdfFormat;
use pyo3::prelude::*;
use pyshacl_turbo_engine::CompiledShapes;

use super::settings::Settings;
use super::{Output, Report, dataset_validation, error, expansion, settings};
use crate::NativeDataset;
use crate::store::{NativeStore, NativeTransaction};

#[pyclass(frozen)]
pub(crate) struct SharedShapes {
    pub(super) inner: Arc<CompiledShapes>,
    pub(super) comparison_host: Arc<dyn shacl_bindings::ComparisonHost>,
}

#[pymethods]
impl SharedShapes {
    #[staticmethod]
    #[pyo3(signature = (program, *, _comparison_policy))]
    pub(super) fn from_compiled(
        py: Python<'_>,
        program: &shacl_bindings::PyCompiledShapes,
        _comparison_policy: Py<shacl_bindings::NativeValueComparisonPolicy>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: program.shared_program(),
            comparison_host: shacl_bindings::comparison_host(py, _comparison_policy)?,
        })
    }

    pub(super) fn shares_program(&self, program: &shacl_bindings::PyCompiledShapes) -> bool {
        Arc::ptr_eq(&self.inner, &program.shared_program())
    }

    #[new]
    #[pyo3(signature = (source, advanced, namespaces, numeric_booleans=true, *, _policy, _comparison_policy))]
    pub(crate) fn new(
        py: Python<'_>,
        source: &str,
        advanced: bool,
        namespaces: Vec<(String, String)>,
        numeric_booleans: bool,
        _policy: Py<shacl_bindings::NativeRegexPolicy>,
        _comparison_policy: Py<shacl_bindings::NativeValueComparisonPolicy>,
    ) -> PyResult<Self> {
        let policy = shacl_bindings::compilation_policy(py, _policy)?;
        let comparison_host = shacl_bindings::comparison_host(py, _comparison_policy)?;
        let result = py.detach(|| {
            let graph = pyshacl_turbo_engine::IndexedShapesSource::parse_graph(
                source,
                RdfFormat::Turtle,
                None,
            )?;
            Ok(Self {
                comparison_host,
                inner: Arc::new(CompiledShapes::compile_with_host_policy(
                    graph,
                    advanced,
                    numeric_booleans,
                    &namespaces,
                    shacl_bindings::python_decimal_digit,
                    policy.scalar(),
                    policy.resolver(),
                )?),
            })
        });
        policy.finish_with(result, error)
    }

    pub(crate) fn expand(
        &self,
        py: Python<'_>,
        dataset: &NativeDataset,
        settings: &str,
        inplace: bool,
    ) -> PyResult<expansion::Output> {
        self.expand_dataset(py, dataset, settings, inplace)
    }

    pub(crate) fn validate(
        &self,
        py: Python<'_>,
        dataset: &NativeDataset,
        settings: &str,
        inplace: bool,
    ) -> PyResult<Output> {
        self.validate_dataset(py, dataset, settings, inplace)
    }

    pub(super) fn prepare_dataset(
        &self,
        py: Python<'_>,
        dataset: Py<NativeDataset>,
        settings: &str,
        inplace: bool,
    ) -> PyResult<dataset_validation::PreparedDatasetValidation> {
        self.prepare_dataset_validation(py, dataset, settings, inplace)
    }

    #[pyo3(signature = (store, settings, inplace, graph=None))]
    fn validate_store(
        &self,
        py: Python<'_>,
        store: &NativeStore,
        settings: &str,
        inplace: bool,
        graph: Option<&str>,
    ) -> PyResult<Output> {
        let captured = Settings::capture(py, settings, self.comparison_host.as_ref())?;
        let graph = graph
            .map(crate::store::parse_graph_scope)
            .transpose()
            .map_err(crate::error)?;
        let settings::Captured {
            options,
            inference,
            publisher,
        } = captured;
        if let Some(inference) = inference {
            if inplace {
                let result = py.detach(|| {
                    store.prepare_inferred_validation(self.inner.clone(), options, graph, inference)
                });
                let mut prepared = settings::Captured::complete(publisher, py, result)?;
                let output = prepared.output()?;
                prepared.accept(py)?;
                return Ok(output);
            }
            return settings::Captured::complete(
                publisher,
                py,
                py.detach(|| {
                    store.validate_inferred_readonly(&self.inner, options, graph, inference)
                }),
            )
            .map(Report::into_output);
        }
        if inplace {
            let result = py.detach(|| store.prepare_validation(self.inner.clone(), options, graph));
            let mut prepared = settings::Captured::complete(publisher, py, result)?;
            let output = prepared.output()?;
            prepared.accept(py)?;
            return Ok(output);
        }
        settings::Captured::complete(
            publisher,
            py,
            py.detach(|| store.validate_shapes(self.inner.clone(), options, false, graph)),
        )
        .map(Report::into_output)
    }

    #[pyo3(signature = (transaction, settings, graph=None))]
    fn prepare_rules_transaction(
        &self,
        py: Python<'_>,
        transaction: &NativeTransaction,
        settings: &str,
        graph: Option<&str>,
    ) -> PyResult<crate::store::PreparedRules> {
        let parsed = Settings::capture_rules(py, settings, self.comparison_host.as_ref()).and_then(
            |(options, publisher)| {
                graph
                    .map(crate::store::parse_graph_scope)
                    .transpose()
                    .map(|graph| (options, graph, publisher))
                    .map_err(crate::error)
            },
        );
        let (options, graph, publisher) = match parsed {
            Ok(parsed) => parsed,
            Err(error) => {
                py.detach(|| transaction.abort_validation())
                    .map_err(crate::error)?;
                return Err(error);
            }
        };
        settings::Captured::complete(
            publisher,
            py,
            py.detach(|| transaction.prepare_rules(self.inner.clone(), options, graph)),
        )
    }

    #[pyo3(signature = (transaction, settings, inplace, graph=None))]
    fn prepare_transaction(
        &self,
        py: Python<'_>,
        transaction: &NativeTransaction,
        settings: &str,
        inplace: bool,
        graph: Option<&str>,
    ) -> PyResult<crate::store::PreparedValidation> {
        let parsed =
            Settings::capture(py, settings, self.comparison_host.as_ref()).and_then(|captured| {
                graph
                    .map(crate::store::parse_graph_scope)
                    .transpose()
                    .map(|graph| (captured, graph))
                    .map_err(crate::error)
            });
        let (captured, graph) = match parsed {
            Ok(parsed) => parsed,
            Err(error) => {
                py.detach(|| transaction.abort_validation())
                    .map_err(crate::error)?;
                return Err(error);
            }
        };
        let settings::Captured {
            options,
            inference,
            publisher,
        } = captured;
        let result = py.detach(|| match inference {
            Some(inference) => transaction.prepare_inferred_validation(
                self.inner.clone(),
                options,
                inplace,
                graph,
                inference,
            ),
            None => transaction.prepare_validation(self.inner.clone(), options, inplace, graph),
        });
        settings::Captured::complete(publisher, py, result)
    }

    fn validate_transaction(
        &self,
        py: Python<'_>,
        transaction: &NativeTransaction,
        settings: &str,
        inplace: bool,
    ) -> PyResult<Output> {
        let mut prepared = self.prepare_transaction(py, transaction, settings, inplace, None)?;
        let output = prepared.output()?;
        prepared.accept(py)?;
        Ok(output)
    }
}
