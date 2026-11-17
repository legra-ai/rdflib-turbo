//! The Python-visible native store handle and its native operations.

use std::sync::{Arc, Mutex};

use oxigraph::model::GraphNameRef;
use oxigraph::store::Store;

use super::operations::{Operation, runtime};
use super::session::{Control, Session, lock};
use super::typed::{self, TermTuple};
use super::typed_view::{Position, PreparedTerm};
use super::{bindings, output, pattern, prepared, read_input};

#[pyo3::pyclass(frozen, module = "rdflib_turbo._native")]
pub(crate) struct NativeStore {
    pub(super) control: Arc<Mutex<Control>>,
}

impl NativeStore {
    pub(super) fn snapshot(&self) -> crate::Result<Store> {
        lock(&self.control)?.read()
    }

    pub(super) fn mutate(&self, operation: Operation) -> crate::Result<()> {
        Session::execute_inline(self.control.clone(), operation).map(|_| ())
    }

    pub(super) fn prepared_query_output(
        &self,
        source: &str,
        graph: Option<&str>,
        union: bool,
        bindings: Option<&str>,
    ) -> crate::Result<(String, String)> {
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
    }

    pub(super) fn query_text_output(
        &self,
        source: &str,
        default_union: bool,
        bindings: Option<&str>,
    ) -> crate::Result<(String, String)> {
        let store = self.snapshot()?;
        let (query, kind) = output::prepare_query(source, default_union, bindings.is_some())?;
        let query = bindings::apply(query, bindings)?;
        output::query_output(query.on_store(&store).execute()?, kind)
    }

    pub(super) fn graph_query_output(
        &self,
        source: &str,
        graph: &str,
        bindings: Option<&str>,
    ) -> crate::Result<(String, String)> {
        let store = self.snapshot()?;
        let (mut query, kind) = output::prepare_query(source, false, bindings.is_some())?;
        if query.dataset().is_default_dataset() {
            query
                .dataset_mut()
                .set_default_graph(vec![pattern::graph(graph)?]);
        }
        let query = bindings::apply(query, bindings)?;
        output::query_output(query.on_store(&store).execute()?, kind)
    }

    pub(super) fn graph_nt_output(
        &self,
        graph: Option<TermTuple>,
    ) -> crate::Result<output::GraphNtOutput> {
        let graph = typed::graph(graph)?;
        output::dump_graph_nt(self.snapshot()?.generalized_quads_for_pattern(
            None,
            None,
            None,
            Some(graph.as_ref()),
        ))
    }

    pub(super) fn named_graphs_json(&self) -> crate::Result<String> {
        let graphs = self
            .snapshot()?
            .named_graphs()
            .map(|graph| graph.map(|g| g.to_string()))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(runtime)?;
        serde_json::to_string(&graphs).map_err(runtime)
    }

    pub(super) fn has_named_graph(&self, source: &read_input::TermTuple) -> crate::Result<bool> {
        let store = self.snapshot()?;
        let graph = PreparedTerm::new(read_input::term_view(source), Position::GraphName)?;
        store
            .contains_named_graph(graph.graph_name_ref())
            .map_err(runtime)
    }

    pub(super) fn graph_size(&self, graph: &Option<read_input::TermTuple>) -> crate::Result<usize> {
        let store = self.snapshot()?;
        let graph = read_input::view(graph)
            .map(|source| PreparedTerm::new(source, Position::GraphName))
            .transpose()?;
        store
            .graph_len(graph.as_ref().map_or(GraphNameRef::DefaultGraph, |graph| {
                graph.graph_name_ref().into()
            }))
            .map_err(runtime)
    }

    pub(super) fn matching_quads(&self, source: &str) -> crate::Result<String> {
        let pattern = pattern::Pattern::parse(source)?;
        let store = self.snapshot()?;
        pattern::output(store.generalized_quads_for_pattern(
            pattern.subject.as_ref().map(|v| v.as_ref()),
            pattern.predicate.as_ref().map(|v| v.as_ref()),
            pattern.object.as_ref().map(|v| v.as_ref()),
            pattern.graph.as_ref().map(|v| v.as_ref()),
        ))
    }

    pub(super) fn pattern_count(&self, source: &str) -> crate::Result<usize> {
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
    }
}
