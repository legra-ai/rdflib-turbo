//! The `#[pymethods]` boundary of [`NativeStore`].

use std::sync::Arc;

use pyo3::prelude::*;

use super::native_store::NativeStore;
use super::nt_python_output::nt_python_output;
use super::operations::{Operation, runtime};
use super::session::{Session, lock};
use super::shacl_read;
use super::transaction::NativeTransaction;
use super::typed::{QuadInput, TermTuple};
use super::{
    changes, context_read, fork, import, input_terms, output, pattern, prepared_changes, quad_read,
    read_input, term_output, union_read,
};
use crate::{error, format};

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
        self.prepare_inference_changes_for(
            py,
            mode,
            graphs,
            factories,
            options,
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
        self.infer_changes_for(py, mode, graphs, factories, options, prior_error_messages)
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
    pub(super) fn compile_shapes(
        &self,
        py: Python<'_>,
        advanced: bool,
        numeric_booleans: bool,
        namespaces: Vec<(String, String)>,
        graph: Option<&str>,
        _policy: Py<shacl_bindings::NativeRegexPolicy>,
    ) -> PyResult<shacl_bindings::PyCompiledShapes> {
        let graph = graph.map(pattern::graph).transpose().map_err(error)?;
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
        self.contains_term_tuples(py, subject, predicate, object, graph)
    }

    fn contains_pattern<'py>(
        &self,
        py: Python<'py>,
        pattern: &Bound<'py, PyAny>,
        context: &Bound<'py, PyAny>,
        inputs: Py<input_terms::NativeInputTerms>,
    ) -> PyResult<bool> {
        self.contains_staged_pattern(py, pattern, context, inputs)
    }

    fn matches_pattern<'py>(
        &self,
        py: Python<'py>,
        pattern: &Bound<'py, PyAny>,
        context: &Bound<'py, PyAny>,
        inputs: Py<input_terms::NativeInputTerms>,
    ) -> PyResult<bool> {
        self.matches_staged_pattern(py, pattern, context, inputs)
    }

    #[pyo3(signature = (pattern, factories, *, scope=None))]
    fn read_union(
        &self,
        py: Python<'_>,
        pattern: read_input::Pattern,
        factories: Py<term_output::NativeTermFactories>,
        scope: Option<(String, Option<read_input::TermTuple>)>,
    ) -> PyResult<union_read::FirstBatch> {
        self.first_union_batch(py, pattern, factories, scope)
    }

    fn read_contexts(
        &self,
        py: Python<'_>,
        pattern: read_input::Pattern,
        factories: Py<term_output::NativeTermFactories>,
    ) -> PyResult<context_read::FirstBatch> {
        self.first_context_batch(py, pattern, factories)
    }

    fn read_quads(
        &self,
        py: Python<'_>,
        pattern: read_input::Pattern,
        scope: (String, Option<read_input::TermTuple>),
        factories: Py<term_output::NativeTermFactories>,
    ) -> PyResult<quad_read::FirstBatch> {
        self.first_quad_batch(py, pattern, scope, factories)
    }

    fn read_pattern(
        &self,
        py: Python<'_>,
        pattern: read_input::Pattern,
        graph: Option<read_input::TermTuple>,
        mask: u8,
        factories: Py<term_output::NativeTermFactories>,
    ) -> PyResult<term_output::FirstBatch> {
        self.first_pattern_batch(py, pattern, graph, mask, factories)
    }

    #[new]
    #[pyo3(signature = (path=None))]
    pub(super) fn new(py: Python<'_>, path: Option<String>) -> PyResult<Self> {
        py.detach(|| Self::open(path)).map_err(error)
    }

    #[pyo3(signature = (graph=None))]
    pub(super) fn fork_graph(&self, py: Python<'_>, graph: Option<String>) -> PyResult<Self> {
        py.detach(|| {
            let graph = graph.as_deref().map(pattern::graph).transpose()?;
            let store = self.snapshot()?;
            fork::copy_graph(&shacl_read::CommittedGraph(store.read_snapshot()), graph)
        })
        .map(Self::from_store)
        .map_err(error)
    }

    pub(super) fn ensure_open(&self) -> PyResult<()> {
        self.snapshot().map(|_| ()).map_err(error)
    }

    pub(super) fn close(&self, py: Python<'_>) -> PyResult<()> {
        py.detach(|| lock(&self.control)?.close()).map_err(error)
    }

    pub(super) fn __len__(&self, py: Python<'_>) -> PyResult<usize> {
        py.detach(|| self.snapshot()?.len().map_err(runtime))
            .map_err(error)
    }

    pub(super) fn transaction(&self, py: Python<'_>) -> PyResult<NativeTransaction> {
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
        self.infer_inline(
            py,
            mode,
            destination,
            axiomatic_triples,
            datatype_axioms,
            prior_error_messages,
        )
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
        self.infer_graph_inline(
            py,
            mode,
            graphs,
            axiomatic_triples,
            datatype_axioms,
            prior_error_messages,
        )
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
        py.detach(|| self.prepared_query_output(source, graph, union, bindings))
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
        py.detach(|| self.query_text_output(source, default_union, bindings))
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
        let text = py.detach(|| self.graph_nt_output(graph)).map_err(error)?;
        nt_python_output(py, text, as_bytes)
    }

    fn named_graphs(&self, py: Python<'_>) -> PyResult<String> {
        py.detach(|| self.named_graphs_json()).map_err(error)
    }

    fn contains_named_graph(
        &self,
        py: Python<'_>,
        source: read_input::TermTuple,
    ) -> PyResult<bool> {
        py.detach(|| self.has_named_graph(&source)).map_err(error)
    }

    #[pyo3(signature = (graph))]
    fn graph_len(&self, py: Python<'_>, graph: Option<read_input::TermTuple>) -> PyResult<usize> {
        py.detach(|| self.graph_size(&graph)).map_err(error)
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
        py.detach(|| self.matching_quads(source)).map_err(error)
    }

    fn count_pattern(&self, py: Python<'_>, source: &str) -> PyResult<usize> {
        py.detach(|| self.pattern_count(source)).map_err(error)
    }

    #[pyo3(signature = (source, graph, bindings=None))]
    fn query_graph(
        &self,
        py: Python<'_>,
        source: &str,
        graph: &str,
        bindings: Option<&str>,
    ) -> PyResult<(String, String)> {
        py.detach(|| self.graph_query_output(source, graph, bindings))
            .map_err(error)
    }
}
