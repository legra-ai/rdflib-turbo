//! Native storage and transaction handles, owned by this extension only.
mod bindings;
mod changes;
#[cfg(test)]
mod changes_tests;
mod context_read;
mod fork;
mod import;
#[cfg(test)]
mod import_tests;
mod input_terms;
mod nt_format;
#[cfg(test)]
mod nt_output_tests;
mod operations;
pub(crate) mod output;
mod pattern;
mod prepared;
mod prepared_changes;
mod quad_read;
mod quad_stream;
mod read;
mod read_input;
#[cfg(test)]
mod read_quads_tests;
mod registration;
mod session;
mod shacl;
mod shacl_inference;
mod shacl_read;
mod shacl_rules;
mod term_output;
mod transaction;
mod typed;
mod typed_view;
mod union_read;

use std::ffi::CString;
use std::sync::{Arc, Mutex};

use oxigraph::model::GraphNameRef;
use oxigraph::store::Store;
use pyo3::IntoPyObjectExt;
use pyo3::exceptions::{PyException, PyUserWarning};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyString};

use crate::{error, format};
use operations::{Operation, runtime};
pub(crate) use pattern::graph as parse_graph_scope;
pub(crate) use registration::register;
use session::{Control, Session, lock};
pub(crate) use shacl::PreparedValidation;
pub(crate) use shacl_rules::PreparedRules;
pub(crate) use transaction::NativeTransaction;
use typed::{QuadInput, TermTuple};
use typed_view::{Position, PreparedTerm};

fn nt_python_output(
    py: Python<'_>,
    output: output::GraphNtOutput,
    as_bytes: bool,
) -> PyResult<Py<PyAny>> {
    let output = match output {
        output::GraphNtOutput::WithWarnings(warnings, terminal) => {
            for warning in warnings {
                let value = PyString::new(py, &warning.value).repr()?;
                let datatype = PyString::new(py, &warning.datatype).repr()?;
                let message = format!(
                    "Serializing weird numerical rdflib.term.Literal({}, datatype=rdflib.term.URIRef({}))",
                    value.to_str()?,
                    datatype.to_str()?,
                );
                let message = CString::new(message).expect("Python repr escapes null characters");
                PyErr::warn(py, &py.get_type::<PyUserWarning>(), &message, 1)?;
            }
            *terminal
        }
        terminal => terminal,
    };
    let text = match output {
        output::GraphNtOutput::Document(text) => text,
        output::GraphNtOutput::WithWarnings(..) => {
            unreachable!("Only one warning envelope is emitted")
        }
        output::GraphNtOutput::InvalidUri(uri) => {
            return Err(PyException::new_err(format!(
                "\"{uri}\" does not look like a valid URI, I cannot serialize this as N3/Turtle. Perhaps you wanted to urlencode it?"
            )));
        }
    };
    if as_bytes {
        Ok(PyBytes::new(py, text.as_bytes()).into_any().unbind())
    } else {
        text.into_py_any(py)
    }
}

#[pyclass(frozen, module = "rdflib_turbo._native")]
pub(crate) struct NativeStore {
    control: Arc<Mutex<Control>>,
}

impl NativeStore {
    fn snapshot(&self) -> crate::Result<Store> {
        lock(&self.control)?.read()
    }

    fn mutate(&self, operation: Operation) -> crate::Result<()> {
        Session::execute_inline(self.control.clone(), operation).map(|_| ())
    }
}

#[pymethods]
impl NativeStore {
    #[pyo3(signature = (mode, graphs, factories, *, options=(false, false), prior_error_messages=None))]
    fn _prepare_inference_changes(
        &self,
        py: Python<'_>,
        mode: String,
        graphs: (Option<String>, String),
        factories: Py<term_output::NativeTermFactories>,
        options: (bool, bool),
        prior_error_messages: Option<Vec<String>>,
    ) -> PyResult<prepared_changes::NativePreparedInferenceChanges> {
        prepared_changes::prepare(
            py,
            self,
            mode,
            graphs,
            factories,
            crate::inference::InferenceOptions {
                axiomatic_triples: options.0,
                datatype_axioms: options.1,
            },
            prior_error_messages,
        )
    }

    #[pyo3(signature = (mode, graphs, factories, *, options=(false, false), prior_error_messages=None))]
    fn _infer_changes(
        &self,
        py: Python<'_>,
        mode: String,
        graphs: (Option<String>, String),
        factories: Py<term_output::NativeTermFactories>,
        options: (bool, bool),
        prior_error_messages: Option<Vec<String>>,
    ) -> PyResult<changes::NativeInferenceChanges> {
        changes::infer(
            py,
            self,
            mode,
            graphs,
            factories,
            crate::inference::InferenceOptions {
                axiomatic_triples: options.0,
                datatype_axioms: options.1,
            },
            prior_error_messages,
        )
    }

    #[staticmethod]
    fn from_records(
        py: Python<'_>,
        records: &Bound<'_, PyAny>,
        declared_graphs: &Bound<'_, PyAny>,
        inputs: Py<input_terms::NativeInputTerms>,
    ) -> PyResult<Self> {
        import::from_records(py, records, declared_graphs, inputs.bind(py).get())
    }

    #[pyo3(signature = (advanced, numeric_booleans, namespaces, graph, *, _policy))]
    fn compile_shapes(
        &self,
        py: Python<'_>,
        advanced: bool,
        numeric_booleans: bool,
        namespaces: Vec<(String, String)>,
        graph: Option<&str>,
        _policy: Py<shacl_bindings::NativeRegexPolicy>,
    ) -> PyResult<shacl_bindings::PyCompiledShapes> {
        let graph = graph.map(parse_graph_scope).transpose().map_err(error)?;
        let policy = shacl_bindings::compilation_policy(py, _policy)?;
        let result = py.detach(|| {
            let store = self.snapshot()?;
            let source =
                pyshacl_turbo_engine::IndexedShapesSource::new(store.read_snapshot(), graph);
            let program = pyshacl_turbo_engine::CompiledShapes::compile_with_host_policy(
                source,
                advanced,
                numeric_booleans,
                &namespaces,
                shacl_bindings::python_decimal_digit,
                policy.scalar(),
                policy.resolver(),
            )?;
            Ok::<_, crate::Error>(shacl_bindings::PyCompiledShapes::from_program(program))
        });
        policy.finish_with(result, error)
    }

    fn insert_terms(
        &self,
        py: Python<'_>,
        subject: TermTuple,
        predicate: TermTuple,
        object: TermTuple,
        graph: Option<TermTuple>,
    ) -> PyResult<()> {
        py.detach(|| {
            self.mutate(Operation::InsertTerms(Box::new(QuadInput::new(
                subject, predicate, object, graph,
            ))))
        })
        .map_err(error)
    }

    #[pyo3(signature = (subject, predicate, object, graph))]
    fn contains_terms(
        &self,
        py: Python<'_>,
        subject: read_input::TermTuple,
        predicate: read_input::TermTuple,
        object: read_input::TermTuple,
        graph: Option<read_input::TermTuple>,
    ) -> PyResult<bool> {
        py.detach(|| {
            read::contains_exact(
                &self.snapshot()?,
                read_input::term_view(&subject),
                read_input::term_view(&predicate),
                read_input::term_view(&object),
                read_input::view(&graph),
            )
        })
        .map_err(error)
    }

    fn contains_pattern<'py>(
        &self,
        py: Python<'py>,
        pattern: &Bound<'py, PyAny>,
        context: &Bound<'py, PyAny>,
        inputs: Py<input_terms::NativeInputTerms>,
    ) -> PyResult<bool> {
        let staged = inputs.bind(py).get().stage(py, pattern, context)?;
        let pinned = staged.pin(py)?;
        py.detach(|| {
            read::contains_exact(
                &self.snapshot()?,
                pinned.subject.view(),
                pinned.predicate.view(),
                pinned.object.view(),
                pinned.graph.as_ref().map(input_terms::PinnedTerm::view),
            )
        })
        .map_err(error)
    }

    #[pyo3(signature = (pattern, factories, *, scope=None))]
    fn read_union(
        &self,
        py: Python<'_>,
        pattern: read_input::Pattern,
        factories: Py<term_output::NativeTermFactories>,
        scope: Option<(String, Option<read_input::TermTuple>)>,
    ) -> PyResult<union_read::FirstBatch> {
        let stream = py
            .detach(|| union_read::open(&self.snapshot()?, &pattern, scope.as_ref()))
            .map_err(error)?;
        union_read::first(py, stream, factories)
    }

    fn read_contexts(
        &self,
        py: Python<'_>,
        pattern: read_input::Pattern,
        factories: Py<term_output::NativeTermFactories>,
    ) -> PyResult<context_read::FirstBatch> {
        let stream = py
            .detach(|| context_read::open(&self.snapshot()?, &pattern))
            .map_err(error)?;
        context_read::first(py, stream, factories)
    }

    fn read_quads(
        &self,
        py: Python<'_>,
        pattern: read_input::Pattern,
        scope: (String, Option<read_input::TermTuple>),
        factories: Py<term_output::NativeTermFactories>,
    ) -> PyResult<quad_read::FirstBatch> {
        let stream = py
            .detach(|| quad_read::open(&self.snapshot()?, &pattern, &scope))
            .map_err(error)?;
        quad_read::first(py, stream, factories)
    }

    fn read_pattern(
        &self,
        py: Python<'_>,
        pattern: read_input::Pattern,
        graph: Option<read_input::TermTuple>,
        mask: u8,
        factories: Py<term_output::NativeTermFactories>,
    ) -> PyResult<term_output::FirstBatch> {
        let (subject, predicate, object) = pattern;
        let batch = py
            .detach(|| {
                read::start(
                    &self.snapshot()?,
                    read_input::view(&subject),
                    read_input::view(&predicate),
                    read_input::view(&object),
                    read_input::view(&graph),
                    mask,
                )
            })
            .map_err(error)?;
        term_output::first(py, batch, factories)
    }

    #[new]
    #[pyo3(signature = (path=None))]
    fn new(py: Python<'_>, path: Option<String>) -> PyResult<Self> {
        py.detach(|| Self::open(path)).map_err(error)
    }

    #[pyo3(signature = (graph=None))]
    fn fork_graph(&self, py: Python<'_>, graph: Option<String>) -> PyResult<Self> {
        py.detach(|| {
            let graph = graph.as_deref().map(pattern::graph).transpose()?;
            let store = self.snapshot()?;
            fork::copy_graph(&shacl_read::CommittedGraph(store.read_snapshot()), graph)
        })
        .map(Self::from_store)
        .map_err(error)
    }

    fn ensure_open(&self) -> PyResult<()> {
        self.snapshot().map(|_| ()).map_err(error)
    }

    fn close(&self, py: Python<'_>) -> PyResult<()> {
        py.detach(|| lock(&self.control)?.close()).map_err(error)
    }

    fn __len__(&self, py: Python<'_>) -> PyResult<usize> {
        py.detach(|| self.snapshot()?.len().map_err(runtime))
            .map_err(error)
    }

    fn transaction(&self, py: Python<'_>) -> PyResult<NativeTransaction> {
        py.detach(|| {
            Session::start(self.control.clone()).map(|session| NativeTransaction {
                session: Arc::new(session),
            })
        })
        .map_err(error)
    }

    #[pyo3(signature = (source, rdf_format, base=None))]
    fn load(
        &self,
        py: Python<'_>,
        source: String,
        rdf_format: &str,
        base: Option<String>,
    ) -> PyResult<()> {
        let format = format(rdf_format)?;
        py.detach(|| {
            self.mutate(Operation::Load {
                source,
                format,
                base,
            })
        })
        .map_err(error)
    }

    #[pyo3(signature = (mode, destination=None, *, axiomatic_triples=false, datatype_axioms=false, prior_error_messages=None))]
    fn infer(
        &self,
        py: Python<'_>,
        mode: String,
        destination: Option<String>,
        axiomatic_triples: bool,
        datatype_axioms: bool,
        prior_error_messages: Option<Vec<String>>,
    ) -> PyResult<crate::inference::InferenceOutcome> {
        let (request, publisher) = crate::inference::InferenceRequest::capture(
            py,
            prior_error_messages,
            crate::inference::InferenceOptions {
                axiomatic_triples,
                datatype_axioms,
            },
        )?;
        let result = py.detach(|| {
            match Session::execute_inline(
                self.control.clone(),
                Operation::Infer(mode, destination, request),
            )? {
                operations::Reply::Inference(outcome) => Ok(outcome),
                _ => unreachable!("native inference reply type"),
            }
        });
        publisher.complete(py, result)
    }

    #[pyo3(signature = (mode, graphs, *, axiomatic_triples=false, datatype_axioms=false, prior_error_messages=None))]
    fn infer_graph(
        &self,
        py: Python<'_>,
        mode: String,
        graphs: (Option<String>, String),
        axiomatic_triples: bool,
        datatype_axioms: bool,
        prior_error_messages: Option<Vec<String>>,
    ) -> PyResult<crate::inference::InferenceOutcome> {
        let (source_graph, destination_graph) = graphs;
        let (request, publisher) = crate::inference::InferenceRequest::capture(
            py,
            prior_error_messages,
            crate::inference::InferenceOptions {
                axiomatic_triples,
                datatype_axioms,
            },
        )?;
        let result = py.detach(|| {
            match Session::execute_inline(
                self.control.clone(),
                Operation::InferGraph {
                    mode,
                    source: source_graph,
                    destination: destination_graph,
                    request,
                },
            )? {
                operations::Reply::Inference(outcome) => Ok(outcome),
                _ => unreachable!("native scoped inference reply type"),
            }
        });
        publisher.complete(py, result)
    }

    #[pyo3(signature = (source, bindings=None, graph=None))]
    fn update(
        &self,
        py: Python<'_>,
        source: String,
        bindings: Option<String>,
        graph: Option<String>,
    ) -> PyResult<()> {
        py.detach(|| {
            self.mutate(match graph {
                Some(graph) => Operation::UpdateGraph(source, graph, bindings),
                None => Operation::Update(source, bindings),
            })
        })
        .map_err(error)
    }

    #[pyo3(signature = (source, graph=None, union=false, bindings=None))]
    fn query_prepared(
        &self,
        py: Python<'_>,
        source: &str,
        graph: Option<&str>,
        union: bool,
        bindings: Option<&str>,
    ) -> PyResult<(String, String)> {
        py.detach(|| {
            let store = self.snapshot()?;
            let bindings = bindings::InitialBindings::parse(bindings)?;
            let (mut query, kind) = prepared::prepare(source, union, bindings.names())?;
            if let Some(graph) = graph
                && query.dataset().is_default_dataset()
            {
                query
                    .dataset_mut()
                    .set_default_graph(vec![pattern::graph(graph)?]);
            }
            let query = bindings.apply(query)?;
            output::query_output(query.on_store(&store).execute()?, kind)
        })
        .map_err(error)
    }

    #[pyo3(signature = (source, default_union=false, bindings=None))]
    fn query(
        &self,
        py: Python<'_>,
        source: &str,
        default_union: bool,
        bindings: Option<&str>,
    ) -> PyResult<(String, String)> {
        py.detach(|| {
            let store = self.snapshot()?;
            let (query, kind) = output::prepare_query(source, default_union, bindings.is_some())?;
            let query = bindings::apply(query, bindings)?;
            output::query_output(query.on_store(&store).execute()?, kind)
        })
        .map_err(error)
    }

    fn dump(&self, py: Python<'_>, rdf_format: &str) -> PyResult<String> {
        let format = format(rdf_format)?;
        py.detach(|| output::dump(self.snapshot()?.iter(), format))
            .map_err(error)
    }

    #[pyo3(signature = (graph, as_bytes=false))]
    fn _dump_graph_nt(
        &self,
        py: Python<'_>,
        graph: Option<TermTuple>,
        as_bytes: bool,
    ) -> PyResult<Py<PyAny>> {
        let text = py
            .detach(|| {
                let graph = typed::graph(graph)?;
                output::dump_graph_nt(self.snapshot()?.generalized_quads_for_pattern(
                    None,
                    None,
                    None,
                    Some(graph.as_ref()),
                ))
            })
            .map_err(error)?;
        nt_python_output(py, text, as_bytes)
    }

    fn named_graphs(&self, py: Python<'_>) -> PyResult<String> {
        py.detach(|| {
            let graphs = self
                .snapshot()?
                .named_graphs()
                .map(|graph| graph.map(|g| g.to_string()))
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(runtime)?;
            serde_json::to_string(&graphs).map_err(runtime)
        })
        .map_err(error)
    }

    fn contains_named_graph(
        &self,
        py: Python<'_>,
        source: read_input::TermTuple,
    ) -> PyResult<bool> {
        py.detach(|| {
            let store = self.snapshot()?;
            let graph = PreparedTerm::new(read_input::term_view(&source), Position::GraphName)?;
            store
                .contains_named_graph(graph.graph_name_ref())
                .map_err(runtime)
        })
        .map_err(error)
    }

    #[pyo3(signature = (graph))]
    fn graph_len(&self, py: Python<'_>, graph: Option<read_input::TermTuple>) -> PyResult<usize> {
        py.detach(|| {
            let store = self.snapshot()?;
            let graph = read_input::view(&graph)
                .map(|source| PreparedTerm::new(source, Position::GraphName))
                .transpose()?;
            store
                .graph_len(graph.as_ref().map_or(GraphNameRef::DefaultGraph, |graph| {
                    graph.graph_name_ref().into()
                }))
                .map_err(runtime)
        })
        .map_err(error)
    }

    fn change_graph(&self, py: Python<'_>, source: String, remove: bool) -> PyResult<()> {
        py.detach(|| self.mutate(Operation::Graph(source, remove)))
            .map_err(error)
    }

    fn remove_pattern(&self, py: Python<'_>, source: String) -> PyResult<()> {
        py.detach(|| self.mutate(Operation::Remove(source)))
            .map_err(error)
    }

    fn match_quads(&self, py: Python<'_>, source: &str) -> PyResult<String> {
        py.detach(|| {
            let pattern = pattern::Pattern::parse(source)?;
            let store = self.snapshot()?;
            pattern::output(store.generalized_quads_for_pattern(
                pattern.subject.as_ref().map(|v| v.as_ref()),
                pattern.predicate.as_ref().map(|v| v.as_ref()),
                pattern.object.as_ref().map(|v| v.as_ref()),
                pattern.graph.as_ref().map(|v| v.as_ref()),
            ))
        })
        .map_err(error)
    }

    fn count_pattern(&self, py: Python<'_>, source: &str) -> PyResult<usize> {
        py.detach(|| {
            let pattern = pattern::Pattern::parse(source)?;
            let store = self.snapshot()?;
            pattern.count(
                |graph| store.graph_len(graph).map_err(runtime),
                || {
                    store.generalized_quads_for_pattern(
                        pattern.subject.as_ref().map(|v| v.as_ref()),
                        pattern.predicate.as_ref().map(|v| v.as_ref()),
                        pattern.object.as_ref().map(|v| v.as_ref()),
                        pattern.graph.as_ref().map(|v| v.as_ref()),
                    )
                },
            )
        })
        .map_err(error)
    }

    #[pyo3(signature = (source, graph, bindings=None))]
    fn query_graph(
        &self,
        py: Python<'_>,
        source: &str,
        graph: &str,
        bindings: Option<&str>,
    ) -> PyResult<(String, String)> {
        py.detach(|| {
            let store = self.snapshot()?;
            let (mut query, kind) = output::prepare_query(source, false, bindings.is_some())?;
            if query.dataset().is_default_dataset() {
                query
                    .dataset_mut()
                    .set_default_graph(vec![pattern::graph(graph)?]);
            }
            let query = bindings::apply(query, bindings)?;
            output::query_output(query.on_store(&store).execute()?, kind)
        })
        .map_err(error)
    }
}

#[cfg(test)]
mod session_tests;

#[cfg(all(test, feature = "allocation-profiling"))]
mod read_input_tests;

#[cfg(test)]
mod update_scope_tests;

#[cfg(test)]
mod generalized_binding_tests;
