//! SHACL executes where its dataset or transaction is owned. No Rust pointer crosses a DSO.
use std::fmt;
use std::sync::Arc;

use oxigraph::io::RdfFormat;
use oxigraph::model::{NamedNode, Term};
use pyo3::exceptions::{PyNotImplementedError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyshacl_turbo_engine::{CompiledShapes, Error, Options, ValidationReport, parse_graph};
use serde::de::{MapAccess, Visitor, value::MapAccessDeserializer};
use serde::{Deserialize, Deserializer};

use crate::NativeDataset;
use crate::store::{NativeStore, NativeTransaction};

mod report;
pub(crate) use report::Report;

pyo3::create_exception!(_native, SharedValidationFailure, PyValueError);

pub(crate) fn error(error: Error) -> PyErr {
    match error {
        Error::ValidationFailure(_) => SharedValidationFailure::new_err(error.to_string()),
        Error::Unsupported(_) => PyNotImplementedError::new_err(error.to_string()),
        Error::Invalid(_) => PyValueError::new_err(error.to_string()),
        _ => PyRuntimeError::new_err(error.to_string()),
    }
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Settings {
    iterate_rules: bool,
    max_rule_iterations: usize,
    max_validation_depth: usize,
    allow_infos: bool,
    allow_warnings: bool,
    abort_on_first: bool,
    focus_nodes: Vec<String>,
    use_shapes: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        let defaults = Options::default();
        Self {
            iterate_rules: defaults.iterate_rules,
            max_rule_iterations: defaults.max_rule_iterations,
            max_validation_depth: defaults.max_validation_depth,
            allow_infos: defaults.allow_infos,
            allow_warnings: defaults.allow_warnings,
            abort_on_first: defaults.abort_on_first,
            focus_nodes: Vec::new(),
            use_shapes: Vec::new(),
        }
    }
}

impl Settings {
    fn options(self) -> PyResult<Options> {
        fn nodes(values: Vec<String>) -> PyResult<Vec<Term>> {
            values
                .into_iter()
                .map(|value| {
                    NamedNode::new(value)
                        .map(Term::from)
                        .map_err(|e| PyValueError::new_err(e.to_string()))
                })
                .collect()
        }
        Ok(Options {
            iterate_rules: self.iterate_rules,
            max_rule_iterations: self.max_rule_iterations,
            max_validation_depth: self.max_validation_depth,
            allow_infos: self.allow_infos,
            allow_warnings: self.allow_warnings,
            abort_on_first: self.abort_on_first,
            focus_nodes: nodes(self.focus_nodes)?,
            use_shapes: nodes(self.use_shapes)?,
            namespaces: Vec::new(),
        })
    }
}

type Output = (bool, String, String, Vec<String>);

fn output(report: ValidationReport) -> PyResult<Output> {
    Report::try_new(report)
        .map(Report::into_output)
        .map_err(crate::error)
}

struct SettingsMap;
impl<'de> Visitor<'de> for SettingsMap {
    type Value = Settings;
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a validation settings object")
    }
    fn visit_map<A: MapAccess<'de>>(self, map: A) -> std::result::Result<Settings, A::Error> {
        Settings::deserialize(MapAccessDeserializer::new(map))
    }
}

impl Settings {
    fn parse(source: &str) -> PyResult<Options> {
        let mut parser = serde_json::Deserializer::from_str(source);
        let settings = parser
            .deserialize_map(SettingsMap)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        parser
            .end()
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        settings.options()
    }
}

#[pyclass(frozen)]
pub(crate) struct SharedShapes {
    inner: Arc<CompiledShapes>,
}

#[pymethods]
impl SharedShapes {
    #[new]
    #[pyo3(signature = (source, advanced, namespaces, numeric_booleans=true))]
    pub(crate) fn new(
        py: Python<'_>,
        source: &str,
        advanced: bool,
        namespaces: Vec<(String, String)>,
        numeric_booleans: bool,
    ) -> PyResult<Self> {
        py.detach(|| {
            let graph = parse_graph(source, RdfFormat::Turtle, None)?;
            Ok(Self {
                inner: Arc::new(CompiledShapes::compile_with_namespaces(
                    graph,
                    advanced,
                    numeric_booleans,
                    &namespaces,
                )?),
            })
        })
        .map_err(error)
    }

    pub(crate) fn validate(
        &self,
        py: Python<'_>,
        dataset: &NativeDataset,
        settings: &str,
        inplace: bool,
    ) -> PyResult<Output> {
        let mut options = Settings::parse(settings)?;
        py.detach(|| {
            if inplace {
                let mut data = dataset
                    .inner
                    .write()
                    .map_err(|_| PyRuntimeError::new_err("Dataset lock poisoned"))?;
                options.namespaces = data.namespaces.clone();
                let outcome = self
                    .inner
                    .validate_delta(&data.data, options)
                    .map_err(error)?;
                let result = output(outcome.report)?;
                data.extend(outcome.additions.iter());
                Ok(result)
            } else {
                let data = dataset
                    .inner
                    .read()
                    .map_err(|_| PyRuntimeError::new_err("Dataset lock poisoned"))?;
                options.namespaces = data.namespaces.clone();
                output(
                    self.inner
                        .validate_staged(&data.data, options)
                        .map_err(error)?,
                )
            }
        })
    }
    fn validate_store(
        &self,
        py: Python<'_>,
        store: &NativeStore,
        settings: &str,
        inplace: bool,
    ) -> PyResult<Output> {
        let options = Settings::parse(settings)?;
        py.detach(|| store.validate_shapes(self.inner.clone(), options, inplace))
            .map(Report::into_output)
            .map_err(crate::error)
    }

    fn validate_transaction(
        &self,
        py: Python<'_>,
        transaction: &NativeTransaction,
        settings: &str,
        inplace: bool,
    ) -> PyResult<Output> {
        let options = match Settings::parse(settings) {
            Ok(options) => options,
            Err(error) => {
                py.detach(|| transaction.abort_validation())
                    .map_err(crate::error)?;
                return Err(error);
            }
        };
        py.detach(|| transaction.validate_shapes(self.inner.clone(), options, inplace))
            .map(Report::into_output)
            .map_err(crate::error)
    }
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<SharedShapes>()?;
    module.add("SHARED_SHACL_PROTOCOL", 1)?;
    module.add(
        "SHACL_ENGINE_FINGERPRINT",
        pyshacl_turbo_engine::engine_fingerprint(),
    )?;
    module.add(
        "SHACL_ENGINE_REVISION",
        "fbd07aa18ac46edd2795f5fb341c2d8ca2886fd3",
    )?;
    module.add(
        "SharedValidationFailure",
        module.py().get_type::<SharedValidationFailure>(),
    )?;
    Ok(())
}
