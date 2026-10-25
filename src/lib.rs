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
mod regex_policy;
#[cfg(test)]
mod regex_policy_tests;
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

fn serialize(data: &GeneralizedDataset, format: RdfFormat) -> PyResult<String> {
    let mut writer = RdfSerializer::from_format(format).for_writer(Vec::new());
    for quad in data.iter() {
        let quad =
            QuadRef::try_from(quad).map_err(|error| PyValueError::new_err(error.to_string()))?;
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
    data: GeneralizedDataset,
    namespaces: Vec<(String, String)>,
    revision: u64,
}

impl DatasetState {
    fn new(data: impl Into<GeneralizedDataset>, namespaces: Vec<(String, String)>) -> Self {
        Self {
            data: data.into(),
            namespaces,
            revision: 0,
        }
    }

    fn copy_owner(&self) -> Self {
        Self::new(self.data.clone(), self.namespaces.clone())
    }

    fn next_revision(&self) -> Result<u64> {
        self.revision
            .checked_add(1)
            .ok_or_else(|| Error::Runtime("Dataset mutation revision exhausted".into()))
    }

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
    type Target = GeneralizedDataset;
    fn deref(&self) -> &GeneralizedDataset {
        &self.data
    }
}

impl DerefMut for DatasetState {
    fn deref_mut(&mut self) -> &mut GeneralizedDataset {
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
                inner: RwLock::new(data.copy_owner()),
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
        let mut state = self
            .inner
            .write()
            .map_err(|_| PyRuntimeError::new_err("Dataset lock poisoned"))?;
        let revision = state.next_revision().map_err(error)?;
        state.bind(entries);
        state.revision = revision;
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
    let standalone_shacl = PyModule::new(module.py(), "shacl")?;
    shacl_bindings::register(
        &standalone_shacl,
        regex_policy::build(module.py())?,
        std::sync::Arc::new(inference::ComparisonHost::new(module.py())?),
    )?;
    module.add_submodule(&standalone_shacl)?;
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

#[cfg(test)]
mod dataset_revision_tests;

#[cfg(test)]
mod dataset_generalized_tests;
