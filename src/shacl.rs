//! SHACL executes where its dataset or transaction is owned. No Rust pointer crosses a DSO.
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use oxigraph::io::RdfFormat;
use oxigraph::model::{Dataset, GraphNameRef, NamedNode, Term, TermRef};
use pyo3::exceptions::{PyNotImplementedError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyshacl_turbo_engine::{CompiledShapes, Error, Options, ValidationReport, parse_graph};
use serde::de::{MapAccess, Visitor, value::MapAccessDeserializer};
use serde::{Deserialize, Deserializer};

use crate::NativeDataset;
use crate::store::{NativeStore, NativeTransaction};

mod report;
pub(crate) use report::Report;

mod commit;
mod expansion;
use commit::Prepared;

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

type Record = (u8, String, Option<String>, Option<String>);
type QuadRecord = (usize, usize, usize, Option<usize>);
type Batch = (Vec<Record>, Vec<QuadRecord>);
pub(crate) type Output = (bool, Batch, String, Vec<String>);

#[derive(Default)]
struct ReportTerms<'a> {
    records: Vec<Record>,
    indices: HashMap<TermRef<'a>, usize>,
}

impl<'a> ReportTerms<'a> {
    fn encode(data: &'a Dataset) -> Batch {
        let mut terms = Self::default();
        let quads = data
            .iter()
            .map(|quad| {
                let subject = terms.intern(quad.subject.into());
                let predicate = terms.intern(quad.predicate.into());
                let object = terms.intern(quad.object);
                let graph = match quad.graph_name {
                    GraphNameRef::DefaultGraph => None,
                    GraphNameRef::NamedNode(node) => Some(terms.intern(node.into())),
                    GraphNameRef::BlankNode(node) => Some(terms.intern(node.into())),
                };
                (subject, predicate, object, graph)
            })
            .collect();
        (terms.records, quads)
    }

    fn intern(&mut self, term: TermRef<'a>) -> usize {
        if let Some(&index) = self.indices.get(&term) {
            return index;
        }
        let record = match term {
            TermRef::NamedNode(node) => (0, node.as_str().to_owned(), None, None),
            TermRef::BlankNode(node) => (1, node.as_str().to_owned(), None, None),
            TermRef::Literal(literal) => (
                2,
                literal.value().to_owned(),
                (literal.language().is_none() && literal.has_explicit_datatype())
                    .then(|| literal.datatype().as_str().to_owned()),
                literal.language().map(str::to_owned),
            ),
        };
        let index = self.records.len();
        self.records.push(record);
        self.indices.insert(term, index);
        index
    }
}

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
            let revision = data.next_revision()?;
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

    fn prepare_transaction(
        &self,
        py: Python<'_>,
        transaction: &NativeTransaction,
        settings: &str,
        inplace: bool,
    ) -> PyResult<crate::store::PreparedValidation> {
        let options = match Settings::parse(settings) {
            Ok(options) => options,
            Err(error) => {
                py.detach(|| transaction.abort_validation())
                    .map_err(crate::error)?;
                return Err(error);
            }
        };
        py.detach(|| transaction.prepare_validation(self.inner.clone(), options, inplace))
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
    module.add_class::<crate::store::PreparedValidation>()?;
    module.add("SHARED_SHACL_PROTOCOL", 1)?;
    module.add(
        "SHACL_ENGINE_FINGERPRINT",
        pyshacl_turbo_engine::engine_fingerprint(),
    )?;
    module.add(
        "SHACL_ENGINE_REVISION",
        "ce41f79d23561690fb760fda252af0e197187731",
    )?;
    module.add(
        "SharedValidationFailure",
        module.py().get_type::<SharedValidationFailure>(),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use oxigraph::model::{BlankNode, GraphName, Literal, Quad};

    use super::*;

    #[test]
    fn report_terms_keep_exact_literals_and_all_graph_name_kinds() {
        let subject = BlankNode::new("subject").unwrap();
        let predicate = NamedNode::new("urn:p").unwrap();
        let mut data = Dataset::new();
        // bounded: three fixed graph/term forms exercise the transport boundary.
        for (literal, graph) in [
            (Literal::new_simple_literal("word"), GraphName::DefaultGraph),
            (
                Literal::new_typed_literal(
                    "word",
                    NamedNode::new("http://www.w3.org/2001/XMLSchema#string").unwrap(),
                ),
                NamedNode::new("urn:graph").unwrap().into(),
            ),
            (
                Literal::new_language_tagged_literal("mot", "fr").unwrap(),
                BlankNode::new("graph").unwrap().into(),
            ),
        ] {
            data.insert(&Quad::new(
                subject.clone(),
                predicate.clone(),
                literal,
                graph,
            ));
        }
        let (records, quads) = ReportTerms::encode(&data);
        assert_eq!(records.iter().collect::<HashSet<_>>().len(), records.len());
        assert!(records.contains(&(2, "word".into(), None, None)));
        assert!(records.contains(&(
            2,
            "word".into(),
            Some("http://www.w3.org/2001/XMLSchema#string".into()),
            None,
        )));
        assert!(records.contains(&(2, "mot".into(), None, Some("fr".into()))));
        let graph_records = quads
            .iter()
            .filter_map(|quad| quad.3.map(|index| records[index].clone()))
            .collect::<HashSet<_>>();
        assert_eq!(
            graph_records,
            HashSet::from([
                (0, "urn:graph".into(), None, None),
                (1, "graph".into(), None, None),
            ])
        );
        assert_eq!(quads.iter().filter(|quad| quad.3.is_none()).count(), 1);
    }
}

#[cfg(test)]
mod read_lock_tests;
