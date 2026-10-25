//! SHACL executes where its dataset or transaction is owned. No Rust pointer crosses a DSO.
use std::collections::HashMap;
use std::sync::Arc;

use oxigraph::io::RdfFormat;
use oxigraph::model::{Dataset, GraphNameRef, TermRef};
use pyo3::exceptions::{PyNotImplementedError, PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyshacl_turbo_engine::{CompiledShapes, Error, ValidationReport};

use crate::NativeDataset;
use crate::store::{NativeStore, NativeTransaction};

mod settings;
pub(crate) use settings::InferenceStage;
#[cfg(test)]
pub(crate) use settings::Mode as InferenceMode;
use settings::Settings;

mod report;
pub(crate) use report::Report;

mod commit;
mod dataset_validation;
mod expansion;
mod inferred_dataset;
#[cfg(test)]
mod provider_binding_tests;

pyo3::create_exception!(_native, SharedValidationFailure, PyValueError);

pub(crate) fn error(error: Error) -> PyErr {
    match error {
        Error::ValidationFailure(_) => SharedValidationFailure::new_err(error.to_string()),
        Error::ShapeLoad { message, link } => {
            shacl_bindings::ShapeLoadFailure::new_err((message, link))
        }
        Error::ConstraintLoad { message, link } => {
            shacl_bindings::ConstraintLoadFailure::new_err((message, link))
        }
        Error::TypeError(message) => PyTypeError::new_err(message),
        Error::Unsupported(_) => PyNotImplementedError::new_err(error.to_string()),
        Error::Invalid(_) => PyValueError::new_err(error.to_string()),
        _ => PyRuntimeError::new_err(error.to_string()),
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

#[pyclass(frozen)]
pub(crate) struct SharedShapes {
    inner: Arc<CompiledShapes>,
}

#[pymethods]
impl SharedShapes {
    #[staticmethod]
    fn from_compiled(program: &shacl_bindings::PyCompiledShapes) -> Self {
        Self {
            inner: program.shared_program(),
        }
    }

    fn shares_program(&self, program: &shacl_bindings::PyCompiledShapes) -> bool {
        Arc::ptr_eq(&self.inner, &program.shared_program())
    }

    #[new]
    #[pyo3(signature = (source, advanced, namespaces, numeric_booleans=true, *, _policy))]
    pub(crate) fn new(
        py: Python<'_>,
        source: &str,
        advanced: bool,
        namespaces: Vec<(String, String)>,
        numeric_booleans: bool,
        _policy: Py<shacl_bindings::NativeRegexPolicy>,
    ) -> PyResult<Self> {
        let policy = shacl_bindings::compilation_policy(py, _policy)?;
        let result = py.detach(|| {
            let graph = pyshacl_turbo_engine::IndexedShapesSource::parse_graph(
                source,
                RdfFormat::Turtle,
                None,
            )?;
            Ok(Self {
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

    fn prepare_dataset(
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
        let captured = Settings::capture(py, settings)?;
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
        py.detach(|| store.validate_shapes(self.inner.clone(), options, inplace, graph))
            .map(Report::into_output)
            .map_err(crate::error)
    }

    #[pyo3(signature = (transaction, settings, graph=None))]
    fn prepare_rules_transaction(
        &self,
        py: Python<'_>,
        transaction: &NativeTransaction,
        settings: &str,
        graph: Option<&str>,
    ) -> PyResult<crate::store::PreparedRules> {
        let parsed = Settings::parse(settings).and_then(|options| {
            graph
                .map(crate::store::parse_graph_scope)
                .transpose()
                .map(|graph| (options, graph))
                .map_err(crate::error)
        });
        let (options, graph) = match parsed {
            Ok(parsed) => parsed,
            Err(error) => {
                py.detach(|| transaction.abort_validation())
                    .map_err(crate::error)?;
                return Err(error);
            }
        };
        py.detach(|| transaction.prepare_rules(self.inner.clone(), options, graph))
            .map_err(crate::error)
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
        let parsed = Settings::capture(py, settings).and_then(|captured| {
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

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<SharedShapes>()?;
    module.add_class::<dataset_validation::PreparedDatasetValidation>()?;
    module.add_class::<crate::store::PreparedValidation>()?;
    module.add_class::<crate::store::PreparedRules>()?;
    module.add("SHARED_SHACL_PROTOCOL", 1)?;
    module.add(
        "SHACL_ENGINE_FINGERPRINT",
        pyshacl_turbo_engine::engine_fingerprint(),
    )?;
    module.add(
        "SHACL_ENGINE_SOURCE",
        env!("RDFLIB_TURBO_SHACL_ENGINE_SOURCE"),
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

    use oxigraph::model::{BlankNode, GraphName, Literal, NamedNode, Quad};

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
