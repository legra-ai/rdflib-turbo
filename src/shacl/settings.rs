//! Strict validation settings and caller-thread inference configuration capture.
use crate::inference::{InferenceOptions, InferenceRequest, SignalPublisher};
use oxigraph::model::{NamedNode, Term};
use pyo3::exceptions::{PyNotImplementedError, PyValueError};
use pyo3::prelude::*;
use pyshacl_turbo_engine::Options;
use serde::de::{MapAccess, Visitor, value::MapAccessDeserializer};
use serde::{Deserialize, Deserializer};
use std::fmt;

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Mode {
    None,
    Rdfs,
    Owlrl,
    Both,
}

impl Mode {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Rdfs => "rdfs",
            Self::Owlrl => "owlrl",
            Self::Both => "both",
        }
    }
}

pub(crate) struct InferenceStage {
    pub mode: Mode,
    pub request: InferenceRequest,
}

pub(super) struct Captured {
    pub options: Options,
    pub inference: Option<InferenceStage>,
    pub publisher: Option<SignalPublisher>,
}

impl Captured {
    pub(super) fn complete<T>(
        publisher: Option<SignalPublisher>,
        py: Python<'_>,
        result: crate::Result<T>,
    ) -> PyResult<T> {
        match publisher {
            Some(publisher) => publisher.complete(py, result),
            None => result.map_err(crate::error),
        }
    }
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Settings {
    inference: Option<Mode>,
    data_namespaces: Vec<(String, String)>,
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
            inference: None,
            data_namespaces: Vec::new(),
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
            namespaces: self.data_namespaces,
            comparison: pyshacl_turbo_engine::CoreComparisonPolicy::NativeSparql,
        })
    }
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
    fn decode(source: &str) -> PyResult<Self> {
        let mut parser = serde_json::Deserializer::from_str(source);
        let settings = parser
            .deserialize_map(SettingsMap)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        parser
            .end()
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        Ok(settings)
    }
}

impl Settings {
    pub(super) fn parse(source: &str) -> PyResult<Options> {
        let settings = Self::decode(source)?;
        if settings
            .inference
            .is_some_and(|mode| !matches!(mode, Mode::None))
        {
            return Err(PyNotImplementedError::new_err(
                "Native rule expansion and this source adapter do not implement validation pre-inference",
            ));
        }
        settings.options()
    }

    pub(super) fn capture(py: Python<'_>, source: &str) -> PyResult<Captured> {
        let settings = Self::decode(source)?;
        let mode = settings
            .inference
            .filter(|mode| !matches!(mode, Mode::None));
        let options = settings.options()?;
        let (inference, publisher) = match mode {
            Some(mode) => {
                let (request, publisher) =
                    InferenceRequest::capture(py, None, InferenceOptions::default())?;
                (Some(InferenceStage { mode, request }), Some(publisher))
            }
            None => (None, None),
        };
        Ok(Captured {
            options,
            inference,
            publisher,
        })
    }
}
