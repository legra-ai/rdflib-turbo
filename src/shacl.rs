//! SHACL executes where the dataset is owned. No Rust pointer crosses a DSO.
use oxigraph::io::RdfFormat;
use oxigraph::model::{NamedNode, Term};
use pyo3::exceptions::{PyNotImplementedError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyshacl_turbo_engine::{CompiledShapes, Error, Options, ValidationReport, parse_graph};
use serde::Deserialize;

use crate::{NativeDataset, serialize};

pyo3::create_exception!(_native, SharedValidationFailure, PyValueError);

fn error(error: Error) -> PyErr {
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
    Ok((
        report.conforms,
        serialize(&report.graph, RdfFormat::NTriples)?,
        report.text,
        report.recursion_warnings,
    ))
}

#[pyclass(frozen)]
pub(crate) struct SharedShapes {
    inner: CompiledShapes,
}

#[pymethods]
impl SharedShapes {
    #[new]
    #[pyo3(signature = (source, advanced, namespaces, numeric_booleans=true))]
    fn new(
        py: Python<'_>,
        source: &str,
        advanced: bool,
        namespaces: Vec<(String, String)>,
        numeric_booleans: bool,
    ) -> PyResult<Self> {
        py.detach(|| {
            let graph = parse_graph(source, RdfFormat::Turtle, None)?;
            Ok(Self {
                inner: CompiledShapes::compile_with_namespaces(
                    graph,
                    advanced,
                    numeric_booleans,
                    &namespaces,
                )?,
            })
        })
        .map_err(error)
    }

    fn validate(
        &self,
        py: Python<'_>,
        dataset: &NativeDataset,
        settings: &str,
        inplace: bool,
    ) -> PyResult<Output> {
        let object = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(settings)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        let mut options = serde_json::from_value::<Settings>(serde_json::Value::Object(object))
            .map_err(|e| PyValueError::new_err(e.to_string()))?
            .options()?;
        py.detach(|| {
            if !self.inner.has_active_rules(&options) {
                let data = dataset
                    .inner
                    .read()
                    .map_err(|_| PyRuntimeError::new_err("Dataset lock poisoned"))?;
                options.namespaces = data.namespaces.clone();
                return output(
                    self.inner
                        .validate_readonly(&data, options)
                        .map_err(error)?,
                );
            }
            if inplace {
                let mut data = dataset
                    .inner
                    .write()
                    .map_err(|_| PyRuntimeError::new_err("Dataset lock poisoned"))?;
                let mut working = data.clone();
                options.namespaces = working.namespaces.clone();
                let result = output(self.inner.validate(&mut working, options).map_err(error)?)?;
                *data = working;
                Ok(result)
            } else {
                let mut working = dataset
                    .inner
                    .read()
                    .map_err(|_| PyRuntimeError::new_err("Dataset lock poisoned"))?
                    .clone();
                options.namespaces = working.namespaces.clone();
                output(self.inner.validate(&mut working, options).map_err(error)?)
            }
        })
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
        "e565ab9fbc3c4587d736467f7cee18eaf28c30ce",
    )?;
    module.add(
        "SharedValidationFailure",
        module.py().get_type::<SharedValidationFailure>(),
    )?;
    Ok(())
}
