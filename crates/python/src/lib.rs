mod comparison;
pub use comparison::{
    ComparisonHost, ComparisonInvocation, ComparisonPublisher, NativeValueComparisonPolicy,
    capture_comparison, comparison_error, comparison_host,
};
mod canonical;
#[cfg(test)]
mod provider_tests;
mod regex_policy;
#[cfg(test)]
mod regex_policy_tests;
mod shape_terms;
mod terms;

use std::sync::Arc;

use pyo3::exceptions::{PyNotImplementedError, PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyshacl_turbo_engine::oxigraph::io::RdfFormat;
use pyshacl_turbo_engine::oxigraph::{
    io::{RdfParser, RdfSerializer},
    model::*,
};
use pyshacl_turbo_engine::{
    CompiledShapes, Error, Options, RegexPolicy, parse_dataset, parse_graph, serialize_graph,
};

pub use regex_policy::{
    NativeRegexPolicy, RegexCompilationPolicy, compilation_policy, registered_regex_policy,
};

/// Read the current CPython wheel host's immutable Unicode decimal table.
///
/// This scalar-only operation allocates no Python object and needs no GIL.
/// CPython exposes Py_UCS4 -> int as an immutable generated-table lookup.
pub fn python_decimal_digit(ch: char) -> Option<u8> {
    // SAFETY: a Rust char is a valid Unicode scalar. This CPython-specific
    // export accesses no PyObject, refcount, interpreter or error state.
    let value = unsafe { pyo3::ffi::Py_UNICODE_TODECIMAL(ch as u32) };
    u8::try_from(value).ok()
}

pyo3::create_exception!(_native, ValidationFailure, PyValueError);
pyo3::create_exception!(_native, ShapeLoadFailure, PyValueError);
pyo3::create_exception!(_native, ConstraintLoadFailure, PyValueError);

fn error(error: Error) -> PyErr {
    match error {
        Error::Comparison(failure) => Python::attach(|py| comparison_error(py, failure)),
        Error::ValidationFailure(_) => ValidationFailure::new_err(error.to_string()),
        Error::ShapeLoad { message, link } => ShapeLoadFailure::new_err((message, link)),
        Error::ConstraintLoad { message, link } => ConstraintLoadFailure::new_err((message, link)),
        Error::TypeError(message) => PyTypeError::new_err(message),
        Error::Unsupported(_) => PyNotImplementedError::new_err(error.to_string()),
        Error::Invalid(_) => PyValueError::new_err(error.to_string()),
        _ => PyRuntimeError::new_err(error.to_string()),
    }
}

struct InputFormat {
    rdf: RdfFormat,
    store_strings: bool,
}

impl InputFormat {
    fn parse(&self, source: &str, base: Option<&str>) -> pyshacl_turbo_engine::Result<Dataset> {
        if self.store_strings {
            pyshacl_turbo_engine::parse_store_snapshot(source)
        } else {
            parse_dataset(source, self.rdf, base)
        }
    }
}

fn format(name: &str) -> PyResult<InputFormat> {
    let rdf = match name {
        "turtle" | "ttl" => Ok(RdfFormat::Turtle),
        "nt" | "ntriples" => Ok(RdfFormat::NTriples),
        "nquads" | "nq" | "oxigraph-nquads" => Ok(RdfFormat::NQuads),
        "trig" => Ok(RdfFormat::TriG),
        "xml" | "rdfxml" => Ok(RdfFormat::RdfXml),
        "n3" => Ok(RdfFormat::N3),
        "json-ld" | "jsonld" => Ok(RdfFormat::JsonLd {
            profile: Default::default(),
        }),
        _ => Err(PyNotImplementedError::new_err(format!(
            "Unsupported RDF format: {name}"
        ))),
    }?;
    Ok(InputFormat {
        rdf,
        store_strings: name == "oxigraph-nquads",
    })
}

#[pyfunction]
fn parse_rdf(
    py: Python<'_>,
    source: &str,
    rdf_format: &str,
    base: Option<&str>,
) -> PyResult<(String, Vec<(String, String)>)> {
    let format = format(rdf_format)?;
    py.detach(|| {
        let mut parser = RdfParser::from_format(format.rdf);
        if let Some(base) = base {
            parser = parser
                .with_base_iri(base)
                .map_err(|e| PyValueError::new_err(e.to_string()))?;
        }
        let mut parser = parser.for_slice(source.as_bytes());
        let mut writer = RdfSerializer::from_format(RdfFormat::NQuads).for_writer(Vec::new());
        for quad in parser.by_ref() {
            let quad = quad.map_err(|e| PyValueError::new_err(e.to_string()))?;
            let quad = if format.store_strings {
                pyshacl_turbo_engine::store_quad(quad)
            } else {
                quad
            };
            writer
                .serialize_quad(&quad)
                .map_err(|e| PyValueError::new_err(e.to_string()))?;
        }
        let prefixes = parser
            .prefixes()
            .map(|(p, n)| (p.to_owned(), n.to_owned()))
            .collect();
        let output = writer
            .finish()
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok((
            String::from_utf8(output).map_err(|e| PyValueError::new_err(e.to_string()))?,
            prefixes,
        ))
    })
}

#[pyfunction]
fn additions(
    py: Python<'_>,
    before: &Bound<'_, PyAny>,
    rdf_format: &str,
    after: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let direct = !after.is_instance_of::<pyo3::types::PyString>();
    let before = terms::Input::read(before, rdf_format, None)?;
    let after = terms::Input::read(after, if direct { "terms" } else { "nquads" }, None)?;
    let output = py
        .detach(|| {
            let before = before.parse()?;
            let after = after.parse()?;
            let additions = after
                .iter()
                .filter(|q| !before.contains(*q))
                .map(|q| q.into_owned())
                .collect();
            terms::Output::graph(&additions, direct)
        })
        .map_err(error)?;
    output.into_python(py)
}

#[pyfunction]
#[pyo3(signature = (source, rdf_format, mode, destination=None))]
fn infer(
    py: Python<'_>,
    source: &str,
    rdf_format: &str,
    mode: &str,
    destination: Option<&str>,
) -> PyResult<String> {
    let rdf_format = format(rdf_format)?;
    let destination = destination
        .map(NamedNode::new)
        .transpose()
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    py.detach(|| {
        let mut data = rdf_format.parse(source, None)?;
        if let Some(destination) = destination {
            pyshacl_turbo_engine::infer_into(&mut data, mode, destination.into())?;
        } else {
            pyshacl_turbo_engine::infer(&mut data, mode)?;
        }
        serialize_graph(&data)
    })
    .map_err(error)
}

#[pyfunction]
fn inoculate(py: Python<'_>, source: &str, rdf_format: &str, ontology: &str) -> PyResult<String> {
    let rdf_format = format(rdf_format)?;
    py.detach(|| {
        let mut data = rdf_format.parse(source, None)?;
        let ontology = parse_graph(ontology, RdfFormat::NTriples, None)?;
        pyshacl_turbo_engine::inoculate(&mut data, &ontology);
        serialize_graph(&data)
    })
    .map_err(error)
}

#[pyfunction]
fn query(py: Python<'_>, source: &str, rdf_format: &str, query: &str) -> PyResult<String> {
    use pyshacl_turbo_engine::oxigraph::sparql::QueryResults;
    use serde_json::json;
    let rdf_format = format(rdf_format)?;
    py.detach(|| {
        let mut data = rdf_format.parse(source, None)?;
        let default: Vec<_> = data
            .iter()
            .map(|q| Quad::new(q.subject, q.predicate, q.object, GraphName::DefaultGraph))
            .collect();
        data.extend(default);
        let result = match pyshacl_turbo_engine::evaluate_query(&data, &data, query, &[])? {
            QueryResults::Solutions(solutions) => {
                let variables: Vec<_> = solutions
                    .variables()
                    .iter()
                    .map(|v| v.as_str().to_owned())
                    .collect();
                let mut rows = Vec::new();
                for solution in solutions {
                    let row: serde_json::Map<_, _> = solution?
                        .iter()
                        .map(|(variable, value)| {
                            (variable.as_str().to_owned(), query_result_term(value))
                        })
                        .collect();
                    rows.push(row);
                }
                json!({"head": {"vars": variables}, "results": {"bindings": rows}})
            }
            QueryResults::Boolean(value) => json!({"head": {}, "boolean": value}),
            QueryResults::Graph(_) => {
                return Err(Error::Invalid("SELECT or ASK query required".into()));
            }
        };
        Ok(result.to_string())
    })
    .map_err(error)
}

fn query_result_term(term: &Term) -> serde_json::Value {
    use serde_json::json;
    match term {
        Term::NamedNode(value) => json!({"type": "uri", "value": value.as_str()}),
        Term::BlankNode(value) => json!({"type": "bnode", "value": value.as_str()}),
        Term::Literal(value) => {
            let mut result = json!({"type": "literal", "value": value.value()});
            if let Some(language) = value.language() {
                result["xml:lang"] = json!(language);
            } else if value.has_explicit_datatype() {
                // The standard Oxigraph JSON writer omits xsd:string, losing
                // the explicit datatype required by RDFLib term identity.
                result["datatype"] = json!(value.datatype().as_str());
            }
            result
        }
    }
}

#[pyclass(frozen, name = "CompiledShapes")]
pub struct PyCompiledShapes {
    inner: Arc<CompiledShapes>,
}

impl PyCompiledShapes {
    /// Wrap a program compiled from this provider's actual retained source.
    pub fn from_program(program: CompiledShapes) -> Self {
        Self {
            inner: Arc::new(program),
        }
    }

    /// Share this immutable program inside the same Python extension.
    pub fn shared_program(&self) -> Arc<CompiledShapes> {
        Arc::clone(&self.inner)
    }
}

/// Immutable data snapshot with a precomputed default-union view.
#[pyclass(frozen, name = "TargetData")]
struct PyTargetData {
    inner: Dataset,
}

#[pymethods]
impl PyTargetData {
    #[new]
    fn new(
        py: Python<'_>,
        source: &Bound<'_, PyAny>,
        rdf_format: &str,
        base: Option<&str>,
    ) -> PyResult<Self> {
        let input = terms::Input::read(source, rdf_format, base)?;
        py.detach(|| {
            let mut data = input.parse()?;
            let default: Vec<_> = data
                .iter()
                .filter(|quad| !quad.graph_name.is_default_graph())
                .map(|quad| {
                    Quad::new(
                        quad.subject,
                        quad.predicate,
                        quad.object,
                        GraphName::DefaultGraph,
                    )
                })
                .collect();
            data.extend(default);
            Ok(Self { inner: data })
        })
        .map_err(error)
    }
}

type ReportOutput = (bool, Py<PyAny>, String, Option<Py<PyAny>>, Vec<String>);

#[pymethods]
impl PyCompiledShapes {
    #[staticmethod]
    #[pyo3(signature = (source, advanced, numeric_booleans, namespaces, *, _policy))]
    fn from_input(
        py: Python<'_>,
        source: &canonical::ShapesInput,
        advanced: bool,
        numeric_booleans: bool,
        namespaces: Vec<(String, String)>,
        _policy: Py<NativeRegexPolicy>,
    ) -> PyResult<Self> {
        let policy = compilation_policy(py, _policy)?;
        let result = py.detach(|| {
            Ok(Self {
                inner: Arc::new(CompiledShapes::compile_with_host_policy(
                    source.source()?,
                    advanced,
                    numeric_booleans,
                    &namespaces,
                    python_decimal_digit,
                    policy.scalar(),
                    policy.resolver(),
                )?),
            })
        });
        policy.finish(result)
    }

    #[new]
    #[pyo3(signature = (source, rdf_format, base, advanced, numeric_booleans, namespaces, *, _policy))]
    fn new(
        source: &str,
        rdf_format: &str,
        base: Option<&str>,
        advanced: bool,
        numeric_booleans: bool,
        namespaces: Vec<(String, String)>,
        _policy: Bound<'_, NativeRegexPolicy>,
    ) -> PyResult<Self> {
        let py = _policy.py();
        let policy = compilation_policy(py, _policy.unbind())?;
        let rdf_format = format(rdf_format)?;
        let result = py.detach(|| {
            let graph = pyshacl_turbo_engine::IndexedShapesSource::parse_graph(
                source,
                rdf_format.rdf,
                base,
            )?;
            Ok(Self {
                inner: Arc::new(CompiledShapes::compile_with_host_policy(
                    graph,
                    advanced,
                    numeric_booleans,
                    &namespaces,
                    python_decimal_digit,
                    policy.scalar(),
                    policy.resolver(),
                )?),
            })
        });
        policy.finish(result)
    }

    fn shape_nodes(&self) -> Vec<String> {
        self.inner.shape_nodes().map(ToString::to_string).collect()
    }

    fn targeting_index(
        &self,
        py: Python<'_>,
        data: &PyTargetData,
        skip: Vec<String>,
    ) -> PyResult<String> {
        py.detach(|| {
            let skip: std::collections::HashSet<_> = skip.into_iter().collect();
            let skipped = self
                .inner
                .shape_nodes()
                .filter(|node| skip.contains(&node.to_string()))
                .cloned()
                .collect();
            let index = self.inner.targeting_index(&data.inner, &skipped)?;
            let mut output = Dataset::new();
            for (focus, shapes) in index {
                for shape in shapes {
                    let subject = match shape {
                        Term::NamedNode(node) => NamedOrBlankNode::from(node),
                        Term::BlankNode(node) => NamedOrBlankNode::from(node),
                        _ => {
                            return Err(Error::Invalid(
                                "Shape identifier must be an IRI or blank node".into(),
                            ));
                        }
                    };
                    output.insert(&Quad::new(
                        subject,
                        NamedNode::new_unchecked("urn:pyshacl-turbo:target"),
                        focus.clone(),
                        GraphName::DefaultGraph,
                    ));
                }
            }
            serialize_graph(&output)
        })
        .map_err(error)
    }

    fn focus_nodes(
        &self,
        py: Python<'_>,
        node: &str,
        data: &PyTargetData,
        advanced: bool,
    ) -> PyResult<String> {
        py.detach(|| {
            let nodes = self.inner.shape_focus_nodes(node, &data.inner, advanced)?;
            let mut output = Dataset::new();
            for node in nodes {
                output.insert(&Quad::new(
                    NamedNode::new_unchecked("urn:pyshacl-turbo:inspection"),
                    NamedNode::new_unchecked("urn:pyshacl-turbo:focus"),
                    node,
                    GraphName::DefaultGraph,
                ));
            }
            serialize_graph(&output)
        })
        .map_err(error)
    }

    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (source, rdf_format, base, iterate_rules, max_rule_iterations, max_validation_depth, include_data, allow_infos, allow_warnings, abort_on_first, focus_nodes, use_shapes, namespaces, *, _comparison_policy))]
    fn validate(
        &self,
        py: Python<'_>,
        source: &Bound<'_, PyAny>,
        rdf_format: &str,
        base: Option<&str>,
        iterate_rules: bool,
        max_rule_iterations: usize,
        max_validation_depth: usize,
        include_data: bool,
        allow_infos: bool,
        allow_warnings: bool,
        abort_on_first: bool,
        focus_nodes: Vec<String>,
        use_shapes: Vec<String>,
        namespaces: Vec<(String, String)>,
        _comparison_policy: Py<NativeValueComparisonPolicy>,
    ) -> PyResult<ReportOutput> {
        let direct = rdf_format == "terms";
        let input = terms::Input::read(source, rdf_format, base)?;
        let comparison = capture_comparison(py, _comparison_policy)?;
        let authority = comparison.authority();
        let result = py.detach(|| {
            let mut data = input.parse()?;
            let report = self.inner.validate(
                &mut data,
                Options {
                    iterate_rules,
                    max_rule_iterations,
                    max_validation_depth,
                    allow_infos,
                    allow_warnings,
                    abort_on_first,
                    namespaces,
                    comparison: authority,
                    focus_nodes: focus_nodes
                        .into_iter()
                        .map(|s| NamedNode::new(s).map(Term::from))
                        .collect::<std::result::Result<_, _>>()
                        .map_err(|e| Error::Invalid(e.to_string()))?,
                    use_shapes: use_shapes
                        .into_iter()
                        .map(|s| NamedNode::new(s).map(Term::from))
                        .collect::<std::result::Result<_, _>>()
                        .map_err(|e| Error::Invalid(e.to_string()))?,
                },
            )?;
            Ok((
                report.conforms,
                terms::Output::graph(&report.graph, direct)?,
                report.text,
                if include_data {
                    Some(terms::Output::graph(&data, direct)?)
                } else {
                    None
                },
                report.recursion_warnings,
            ))
        });
        let (conforms, report, text, expanded, warnings) = comparison.finish(py, result)?;
        Ok((
            conforms,
            report.into_python(py)?,
            text,
            expanded.map(|value| value.into_python(py)).transpose()?,
            warnings,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (source, rdf_format, base, iterate_rules, max_rule_iterations, max_validation_depth, focus_nodes, use_shapes, additions_only, *, _comparison_policy))]
    fn expand(
        &self,
        py: Python<'_>,
        source: &Bound<'_, PyAny>,
        rdf_format: &str,
        base: Option<&str>,
        iterate_rules: bool,
        max_rule_iterations: usize,
        max_validation_depth: usize,
        focus_nodes: Vec<String>,
        use_shapes: Vec<String>,
        additions_only: bool,
        _comparison_policy: Py<NativeValueComparisonPolicy>,
    ) -> PyResult<(Py<PyAny>, Vec<String>)> {
        let direct = rdf_format == "terms";
        let input = terms::Input::read(source, rdf_format, base)?;
        let comparison = capture_comparison(py, _comparison_policy)?;
        let authority = comparison.authority();
        let result = py.detach(|| {
            let mut data = input.parse()?;
            let options = Options {
                comparison: authority,
                iterate_rules,
                max_rule_iterations,
                max_validation_depth,
                focus_nodes: focus_nodes
                    .into_iter()
                    .map(|s| NamedNode::new(s).map(Term::from))
                    .collect::<std::result::Result<_, _>>()
                    .map_err(|e| Error::Invalid(e.to_string()))?,
                use_shapes: use_shapes
                    .into_iter()
                    .map(|s| NamedNode::new(s).map(Term::from))
                    .collect::<std::result::Result<_, _>>()
                    .map_err(|e| Error::Invalid(e.to_string()))?,
                ..Options::default()
            };
            if additions_only {
                let delta = self.inner.expand_rules_delta(&mut data, options)?;
                Ok((
                    terms::Output::graph(&delta.graph, direct)?,
                    delta.recursion_warnings,
                ))
            } else {
                let warnings = self.inner.expand_rules(&mut data, options)?;
                Ok((terms::Output::graph(&data, direct)?, warnings))
            }
        });
        let (output, warnings) = comparison.finish(py, result)?;
        Ok((output.into_python(py)?, warnings))
    }
}

/// Register SHACL capabilities in the dataset-owning extension.
///
/// Both binding surfaces borrow the provider's engine. Rust allocations never
/// cross independently loaded extensions.
pub fn register(
    module: &Bound<'_, PyModule>,
    regex_policy: Arc<RegexPolicy>,
    comparison_host: Arc<dyn ComparisonHost>,
) -> PyResult<()> {
    if module.hasattr("_REGEX_POLICY")? {
        return Err(PyRuntimeError::new_err(
            "SHACL module is already registered",
        ));
    }
    regex_policy::install(module, regex_policy)?;
    comparison::install(module, comparison_host)?;
    module.add("SHAPES_BATCH_SIZE", canonical::BATCH_SIZE)?;
    module.add(
        "SHACL_ENGINE_FINGERPRINT",
        pyshacl_turbo_engine::engine_fingerprint(),
    )?;
    module.add_class::<PyCompiledShapes>()?;
    module.add_class::<PyTargetData>()?;
    module.add_function(wrap_pyfunction!(parse_rdf, module)?)?;
    module.add_class::<canonical::ShapesInput>()?;
    module.add_function(wrap_pyfunction!(additions, module)?)?;
    module.add_function(wrap_pyfunction!(infer, module)?)?;
    module.add_function(wrap_pyfunction!(inoculate, module)?)?;
    module.add_function(wrap_pyfunction!(query, module)?)?;
    module.add(
        "ConstraintLoadFailure",
        module.py().get_type::<ConstraintLoadFailure>(),
    )?;
    module.add(
        "ShapeLoadFailure",
        module.py().get_type::<ShapeLoadFailure>(),
    )?;
    module.add(
        "ValidationFailure",
        module.py().get_type::<ValidationFailure>(),
    )?;
    Ok(())
}
