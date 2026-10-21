use std::ops::{Deref, DerefMut};
use std::sync::RwLock;

use oxigraph::io::{RdfFormat, RdfParser, RdfSerializer};
use oxigraph::model::*;
use oxigraph::sparql::SparqlEvaluator;
use pyo3::exceptions::{PyNotImplementedError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use serde_json::{Value, json};

mod dataset_inference;
mod errors;
use errors::{Error, Result, error};
mod graph;
mod inference;
mod shacl;
mod store;

fn format(name: &str) -> PyResult<RdfFormat> {
    match name {
        "turtle" | "ttl" => Ok(RdfFormat::Turtle),
        "nt" | "ntriples" => Ok(RdfFormat::NTriples),
        "nquads" | "nq" => Ok(RdfFormat::NQuads),
        "trig" => Ok(RdfFormat::TriG),
        "xml" | "rdfxml" => Ok(RdfFormat::RdfXml),
        "n3" => Ok(RdfFormat::N3),
        "json-ld" | "jsonld" => Ok(RdfFormat::JsonLd {
            profile: Default::default(),
        }),
        _ => Err(PyNotImplementedError::new_err(format!(
            "Unsupported RDF format: {name}"
        ))),
    }
}

fn serialize(data: &Dataset, format: RdfFormat) -> PyResult<String> {
    let mut writer = RdfSerializer::from_format(format).for_writer(Vec::new());
    for quad in data.iter() {
        writer
            .serialize_quad(quad)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
    }
    String::from_utf8(
        writer
            .finish()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?,
    )
    .map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

fn term(term: &Term) -> Value {
    match term {
        Term::NamedNode(value) => json!({"type": "uri", "value": value.as_str()}),
        Term::BlankNode(value) => json!({"type": "bnode", "value": value.as_str()}),
        Term::Literal(value) => {
            let mut result = json!({"type": "literal", "value": value.value()});
            if let Some(language) = value.language() {
                result["xml:lang"] = json!(language);
            } else if value.has_explicit_datatype() {
                result["datatype"] = json!(value.datatype().as_str());
            }
            result
        }
    }
}

/// One runtime-owned exact-term dataset. Locks cover whole native operations.
#[derive(Clone, Default)]
struct DatasetState {
    data: Dataset,
    namespaces: Vec<(String, String)>,
}

impl DatasetState {
    fn bind(&mut self, entries: Vec<(String, String)>) {
        for (prefix, namespace) in entries {
            if let Some((_, value)) = self.namespaces.iter_mut().find(|(key, _)| key == &prefix) {
                *value = namespace;
            } else {
                self.namespaces.push((prefix, namespace));
            }
        }
    }
}

impl Deref for DatasetState {
    type Target = Dataset;
    fn deref(&self) -> &Dataset {
        &self.data
    }
}

impl DerefMut for DatasetState {
    fn deref_mut(&mut self) -> &mut Dataset {
        &mut self.data
    }
}

#[pyclass(frozen)]
struct NativeDataset {
    inner: RwLock<DatasetState>,
}

#[pymethods]
impl NativeDataset {
    #[new]
    fn new() -> Self {
        Self {
            inner: RwLock::new(DatasetState::default()),
        }
    }

    fn __len__(&self) -> PyResult<usize> {
        Ok(self
            .inner
            .read()
            .map_err(|_| PyRuntimeError::new_err("Dataset lock poisoned"))?
            .len())
    }

    fn copy(&self, py: Python<'_>) -> PyResult<Self> {
        py.detach(|| {
            let data = self
                .inner
                .read()
                .map_err(|_| PyRuntimeError::new_err("Dataset lock poisoned"))?;
            Ok(Self {
                inner: RwLock::new(data.clone()),
            })
        })
    }

    #[pyo3(signature = (source, rdf_format, base=None, preserve_blank_nodes=false))]
    fn load(
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
            let mut transaction = state.data.insert_transaction();
            for quad in reader.by_ref() {
                let quad = quad.map_err(|e| PyValueError::new_err(e.to_string()))?;
                transaction.insert(&quad);
            }
            let prefixes = reader
                .prefixes()
                .map(|(p, n)| (p.to_owned(), n.to_owned()))
                .collect();
            transaction.commit();
            state.bind(prefixes);
            Ok(())
        })
    }

    fn namespaces(&self) -> PyResult<Vec<(String, String)>> {
        Ok(self
            .inner
            .read()
            .map_err(|_| PyRuntimeError::new_err("Dataset lock poisoned"))?
            .namespaces
            .clone())
    }

    fn bind_namespaces(&self, entries: Vec<(String, String)>) -> PyResult<()> {
        for (_, namespace) in &entries {
            NamedNode::new(namespace.as_str()).map_err(|e| PyValueError::new_err(e.to_string()))?;
        }
        self.inner
            .write()
            .map_err(|_| PyRuntimeError::new_err("Dataset lock poisoned"))?
            .bind(entries);
        Ok(())
    }

    fn dump(&self, py: Python<'_>, rdf_format: &str) -> PyResult<String> {
        let rdf_format = format(rdf_format)?;
        py.detach(|| {
            let data = self
                .inner
                .read()
                .map_err(|_| PyRuntimeError::new_err("Dataset lock poisoned"))?;
            serialize(&data, rdf_format)
        })
    }

    fn quads_json(&self, py: Python<'_>) -> PyResult<String> {
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
                        term(&q.subject.into_owned().into()),
                        term(&q.predicate.into_owned().into()),
                        term(&q.object.into_owned()),
                        graph
                    ])
                })
                .collect();
            Ok(json!(quads).to_string())
        })
    }

    #[pyo3(signature = (source, default_union=false))]
    fn query(
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
            let union = default_union.then(|| graph::with_union(&data));
            let view = union.as_ref().unwrap_or(&data);
            // SERVICE would require network execution, which this runtime does not enable.
            let result = SparqlEvaluator::new()
                .for_query(parsed)
                .on_queryable_dataset(view)
                .execute()
                .map_err(|e| error(e.into()))?;
            store::output::query_output(result, graph_kind).map_err(error)
        })
    }

    #[pyo3(signature = (mode, destination=None, *, axiomatic_triples=false, datatype_axioms=false, prior_error_messages=None))]
    fn infer(
        &self,
        py: Python<'_>,
        mode: &str,
        destination: Option<&str>,
        axiomatic_triples: bool,
        datatype_axioms: bool,
        prior_error_messages: Option<Vec<String>>,
    ) -> PyResult<inference::InferenceOutcome> {
        if !matches!(mode, "rdfs" | "owlrl" | "both") {
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
            let outcome = if let Some(destination) = destination {
                inference::expand_into(&mut current, mode, destination.into(), request)?
            } else {
                inference::expand(&mut current, mode, request)?
            };
            outcome.bind_namespace(&mut current.namespaces);
            Ok(outcome)
        });
        publisher.complete(py, result)
    }

    #[pyo3(signature = (mode, destination=None, *, axiomatic_triples=false, datatype_axioms=false, prior_error_messages=None))]
    fn infer_delta(
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

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<NativeDataset>()?;
    module.add_class::<inference::InferenceOutcome>()?;
    shacl::register(module)?;
    store::register(module)?;
    Ok(())
}

#[cfg(test)]
mod native_tests;

#[cfg(all(test, feature = "allocation-profiling"))]
mod allocation_tests;

#[cfg(test)]
mod store_tests;

#[cfg(test)]
mod construct_tests;
