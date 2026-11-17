//! The Python-visible in-memory exact-term dataset.
use std::sync::RwLock;

use oxigraph::io::RdfParser;
use oxigraph::model::*;
use oxigraph::sparql::SparqlEvaluator;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use serde_json::{Value, json};

use crate::dataset_output::{serialize, term};
use crate::{DatasetState, Error, error, format, inference, store};

#[pyclass(frozen)]
pub(crate) struct NativeDataset {
    pub(crate) inner: RwLock<DatasetState>,
}

#[pymethods]
impl NativeDataset {
    #[new]
    pub(crate) fn new() -> Self {
        Self {
            inner: RwLock::new(DatasetState::default()),
        }
    }

    pub(crate) fn __len__(&self) -> PyResult<usize> {
        Ok(self
            .inner
            .read()
            .map_err(|_| PyRuntimeError::new_err("Dataset lock poisoned"))?
            .len())
    }

    pub(crate) fn copy(&self, py: Python<'_>) -> PyResult<Self> {
        py.detach(|| {
            let data = self
                .inner
                .read()
                .map_err(|_| PyRuntimeError::new_err("Dataset lock poisoned"))?;
            Ok(Self {
                inner: RwLock::new(data.copy_owner()),
            })
        })
    }

    #[pyo3(signature = (source, rdf_format, base=None, preserve_blank_nodes=false))]
    pub(crate) fn load(
        &self,
        py: Python<'_>,
        source: &str,
        rdf_format: &str,
        base: Option<&str>,
        preserve_blank_nodes: bool,
    ) -> PyResult<()> {
        let rdf_format = format(rdf_format)?;
        py.detach(|| {
            let mut parser = RdfParser::from_format(rdf_format);
            if !preserve_blank_nodes {
                parser = parser.rename_blank_nodes();
            }
            if let Some(base) = base {
                parser = parser
                    .with_base_iri(base)
                    .map_err(|e| PyValueError::new_err(e.to_string()))?;
            }
            let mut reader = parser.for_slice(source.as_bytes());
            let mut state = self
                .inner
                .write()
                .map_err(|_| PyRuntimeError::new_err("Dataset lock poisoned"))?;
            let revision = state.next_revision().map_err(error)?;
            let mut transaction = state.data.transaction();
            for quad in reader.by_ref() {
                let quad = quad.map_err(|e| PyValueError::new_err(e.to_string()))?;
                transaction.insert(quad.as_ref());
            }
            let prefixes = reader
                .prefixes()
                .map(|(p, n)| (p.to_owned(), n.to_owned()))
                .collect();
            transaction.commit();
            state.bind(prefixes);
            state.revision = revision;
            Ok(())
        })
    }

    pub(crate) fn namespaces(&self) -> PyResult<Vec<(String, String)>> {
        Ok(self
            .inner
            .read()
            .map_err(|_| PyRuntimeError::new_err("Dataset lock poisoned"))?
            .namespaces
            .clone())
    }

    pub(crate) fn bind_namespaces(&self, entries: Vec<(String, String)>) -> PyResult<()> {
        for (_, namespace) in &entries {
            NamedNode::new(namespace.as_str()).map_err(|e| PyValueError::new_err(e.to_string()))?;
        }
        let mut state = self
            .inner
            .write()
            .map_err(|_| PyRuntimeError::new_err("Dataset lock poisoned"))?;
        let revision = state.next_revision().map_err(error)?;
        state.bind(entries);
        state.revision = revision;
        Ok(())
    }

    pub(crate) fn dump(&self, py: Python<'_>, rdf_format: &str) -> PyResult<String> {
        let rdf_format = format(rdf_format)?;
        py.detach(|| {
            let data = self
                .inner
                .read()
                .map_err(|_| PyRuntimeError::new_err("Dataset lock poisoned"))?;
            serialize(&data, rdf_format)
        })
    }

    pub(crate) fn quads_json(&self, py: Python<'_>) -> PyResult<String> {
        py.detach(|| {
            let data = self
                .inner
                .read()
                .map_err(|_| PyRuntimeError::new_err("Dataset lock poisoned"))?;
            let quads: Vec<_> = data
                .iter()
                .map(|q| {
                    let graph = match q.graph_name {
                        GraphNameRef::DefaultGraph => Value::Null,
                        GraphNameRef::NamedNode(n) => term(&n.into_owned().into()),
                        GraphNameRef::BlankNode(n) => term(&n.into_owned().into()),
                    };
                    json!([
                        term(&q.subject.into_owned()),
                        term(&q.predicate.into_owned()),
                        term(&q.object.into_owned()),
                        graph
                    ])
                })
                .collect();
            Ok(json!(quads).to_string())
        })
    }

    #[pyo3(signature = (source, default_union=false))]
    pub(crate) fn query(
        &self,
        py: Python<'_>,
        source: &str,
        default_union: bool,
    ) -> PyResult<(String, String)> {
        py.detach(|| {
            let parsed = spargebra::SparqlParser::new()
                .parse_query(source)
                .map_err(|e| PyValueError::new_err(e.to_string()))?;
            let graph_kind = if matches!(parsed, spargebra::Query::Describe { .. }) {
                "DESCRIBE"
            } else {
                "CONSTRUCT"
            };
            let data = self
                .inner
                .read()
                .map_err(|_| PyRuntimeError::new_err("Dataset lock poisoned"))?;
            let union = default_union.then(|| pyshacl_turbo_engine::UnionGraph::new(&data.data));
            let query = SparqlEvaluator::new().for_query(parsed);
            // SERVICE would require network execution, which this runtime does not enable.
            let result = if let Some(view) = &union {
                query
                    .on_queryable_dataset(pyshacl_turbo_engine::QueryGraph::new(view))
                    .execute()
            } else {
                query.on_queryable_dataset(&data.data).execute()
            }
            .map_err(|e| error(e.into()))?;
            store::output::query_output(result, graph_kind).map_err(error)
        })
    }

    #[pyo3(signature = (mode, destination=None, *, axiomatic_triples=false, datatype_axioms=false, prior_error_messages=None))]
    pub(crate) fn infer(
        &self,
        py: Python<'_>,
        mode: &str,
        destination: Option<&str>,
        axiomatic_triples: bool,
        datatype_axioms: bool,
        prior_error_messages: Option<Vec<String>>,
    ) -> PyResult<inference::InferenceOutcome> {
        if !inference::supports_mode(mode) {
            return Err(PyValueError::new_err(format!(
                "Unknown inference mode: {mode}"
            )));
        }
        let destination = destination
            .map(NamedNode::new)
            .transpose()
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        let (request, publisher) = inference::InferenceRequest::capture(
            py,
            prior_error_messages,
            crate::inference::InferenceOptions {
                axiomatic_triples,
                datatype_axioms,
            },
        )?;
        let result = py.detach(|| {
            let mut current = self
                .inner
                .write()
                .map_err(|_| Error::Runtime("Dataset lock poisoned".into()))?;
            let revision = current.next_revision()?;
            let outcome = if let Some(destination) = destination {
                inference::expand_into(&mut current, mode, destination.into(), request)?
            } else {
                inference::expand(&mut current, mode, request)?
            };
            outcome.bind_namespace(&mut current.namespaces);
            current.revision = revision;
            Ok(outcome)
        });
        publisher.complete(py, result)
    }

    #[pyo3(signature = (mode, destination=None, *, axiomatic_triples=false, datatype_axioms=false, prior_error_messages=None))]
    pub(crate) fn infer_delta(
        &self,
        py: Python<'_>,
        mode: &str,
        destination: Option<&str>,
        axiomatic_triples: bool,
        datatype_axioms: bool,
        prior_error_messages: Option<Vec<String>>,
    ) -> PyResult<(Self, inference::InferenceOutcome)> {
        self.expand_delta(
            py,
            mode,
            destination,
            crate::inference::InferenceOptions {
                axiomatic_triples,
                datatype_axioms,
            },
            prior_error_messages,
        )
    }
}
