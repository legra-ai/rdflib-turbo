//! Native storage and transaction handles, owned by this extension only.
mod input_terms;
mod operations;
pub(crate) mod output;
mod pattern;
mod read;
mod read_input;
mod session;
mod shacl;
mod shacl_read;
mod term_output;
mod transaction;
mod typed;
mod typed_view;

use std::sync::{Arc, Mutex};

use oxigraph::model::GraphNameRef;
use oxigraph::store::Store;
use pyo3::prelude::*;

use crate::{error, format};
use operations::{Operation, runtime};
use session::{Control, Session, lock};
pub(crate) use shacl::PreparedValidation;
pub(crate) use transaction::NativeTransaction;
use typed::{QuadInput, TermTuple};
use typed_view::{Position, PreparedTerm};

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
        py.detach(|| {
            let store = match path {
                None => Store::new().map_err(runtime)?,
                Some(path) => {
                    #[cfg(feature = "persistence")]
                    {
                        Store::open(path).map_err(runtime)?
                    }
                    #[cfg(not(feature = "persistence"))]
                    {
                        let _ = path;
                        return Err(crate::Error::Unsupported(
                            "This build has no RocksDB persistence support".into(),
                        ));
                    }
                }
            };
            Ok(Self {
                control: Arc::new(Mutex::new(Control::new(store))),
            })
        })
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

    #[pyo3(signature = (mode, destination=None, *, axiomatic_triples=false, datatype_axioms=false))]
    fn infer(
        &self,
        py: Python<'_>,
        mode: String,
        destination: Option<String>,
        axiomatic_triples: bool,
        datatype_axioms: bool,
    ) -> PyResult<()> {
        py.detach(|| {
            self.mutate(Operation::Infer(
                mode,
                destination,
                crate::inference::InferenceOptions {
                    axiomatic_triples,
                    datatype_axioms,
                },
            ))
        })
        .map_err(error)
    }

    #[pyo3(signature = (mode, source_graph, destination_graph))]
    fn infer_graph(
        &self,
        py: Python<'_>,
        mode: String,
        source_graph: Option<String>,
        destination_graph: String,
    ) -> PyResult<()> {
        py.detach(|| {
            self.mutate(Operation::InferGraph {
                mode,
                source: source_graph,
                destination: destination_graph,
            })
        })
        .map_err(error)
    }

    fn update(&self, py: Python<'_>, source: String) -> PyResult<()> {
        py.detach(|| self.mutate(Operation::Update(source)))
            .map_err(error)
    }

    #[pyo3(signature = (source, default_union=false))]
    fn query(
        &self,
        py: Python<'_>,
        source: &str,
        default_union: bool,
    ) -> PyResult<(String, String)> {
        py.detach(|| {
            let store = self.snapshot()?;
            let (query, kind) = output::prepare_query(source, default_union)?;
            output::query_output(query.on_store(&store).execute()?, kind)
        })
        .map_err(error)
    }

    fn dump(&self, py: Python<'_>, rdf_format: &str) -> PyResult<String> {
        let format = format(rdf_format)?;
        py.detach(|| output::dump(self.snapshot()?.iter(), format))
            .map_err(error)
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
            let graph = PreparedTerm::new(read_input::term_view(&source), Position::Subject)?;
            store
                .contains_named_graph(graph.subject_ref())
                .map_err(runtime)
        })
        .map_err(error)
    }

    #[pyo3(signature = (graph))]
    fn graph_len(&self, py: Python<'_>, graph: Option<read_input::TermTuple>) -> PyResult<usize> {
        py.detach(|| {
            let store = self.snapshot()?;
            let graph = read_input::view(&graph)
                .map(|source| PreparedTerm::new(source, Position::Subject))
                .transpose()?;
            store
                .graph_len(graph.as_ref().map_or(GraphNameRef::DefaultGraph, |graph| {
                    graph.subject_ref().into()
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
            pattern::output(store.quads_for_pattern(
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
                    store.quads_for_pattern(
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

    fn query_graph(&self, py: Python<'_>, source: &str, graph: &str) -> PyResult<(String, String)> {
        py.detach(|| {
            let store = self.snapshot()?;
            let (mut query, kind) = output::prepare_query(source, false)?;
            if query.dataset().is_default_dataset() {
                query
                    .dataset_mut()
                    .set_default_graph(vec![pattern::graph(graph)?]);
            }
            output::query_output(query.on_store(&store).execute()?, kind)
        })
        .map_err(error)
    }
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<term_output::NativeTermFactories>()?;
    module.add_class::<term_output::NativeReadCursor>()?;
    module.add_class::<NativeStore>()?;
    module.add_class::<input_terms::NativeInputTerms>()?;
    module.add_class::<NativeTransaction>()?;
    Ok(())
}

#[cfg(test)]
mod session_tests;

#[cfg(all(test, feature = "allocation-profiling"))]
mod read_input_tests;
