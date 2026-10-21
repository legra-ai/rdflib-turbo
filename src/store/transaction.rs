//! Python transaction handle; its native transaction remains on the session owner.
use pyo3::prelude::*;
use std::sync::Arc;

use super::operations::{Operation, Reply};
use super::session::Session;
use super::typed::{QuadInput, TermTuple};
use crate::{error, format};

#[pyclass(frozen, module = "rdflib_turbo._native")]
pub(crate) struct NativeTransaction {
    pub(super) session: Arc<Session>,
}

#[pymethods]
impl NativeTransaction {
    fn insert_terms(
        &self,
        py: Python<'_>,
        subject: TermTuple,
        predicate: TermTuple,
        object: TermTuple,
        graph: Option<TermTuple>,
    ) -> PyResult<()> {
        py.detach(|| {
            self.session
                .execute(Operation::InsertTerms(Box::new(QuadInput::new(
                    subject, predicate, object, graph,
                ))))
        })
        .map(|_| ())
        .map_err(error)
    }

    fn count_pattern(&self, py: Python<'_>, source: String) -> PyResult<usize> {
        py.detach(|| match self.session.execute(Operation::Count(source))? {
            Reply::Count(count) => Ok(count),
            _ => unreachable!("native graph count reply type"),
        })
        .map_err(error)
    }
    #[pyo3(signature = (source, rdf_format, base, graph, allow_named_graphs=false))]
    fn load_graph(
        &self,
        py: Python<'_>,
        source: String,
        rdf_format: &str,
        base: Option<String>,
        graph: String,
        allow_named_graphs: bool,
    ) -> PyResult<String> {
        let format = match format(rdf_format) {
            Ok(format) => format,
            Err(cause) => {
                py.detach(|| self.session.finish(false)).map_err(error)?;
                return Err(cause);
            }
        };
        py.detach(|| {
            match self.session.execute(Operation::LoadGraph {
                source,
                format,
                base,
                graph,
                allow_named_graphs,
            })? {
                Reply::Text(text) => Ok(text),
                _ => unreachable!("native graph load reply type"),
            }
        })
        .map_err(error)
    }
    fn named_graphs(&self, py: Python<'_>) -> PyResult<String> {
        py.detach(|| match self.session.execute(Operation::Graphs)? {
            Reply::Text(text) => Ok(text),
            _ => unreachable!("native graph names reply type"),
        })
        .map_err(error)
    }

    fn contains_named_graph(&self, py: Python<'_>, source: TermTuple) -> PyResult<bool> {
        py.detach(
            || match self.session.execute(Operation::ContainsGraph(source))? {
                Reply::Bool(value) => Ok(value),
                _ => unreachable!("native graph membership reply type"),
            },
        )
        .map_err(error)
    }

    #[pyo3(signature = (graph))]
    fn graph_len(&self, py: Python<'_>, graph: Option<TermTuple>) -> PyResult<usize> {
        py.detach(|| match self.session.execute(Operation::GraphLen(graph))? {
            Reply::Count(value) => Ok(value),
            _ => unreachable!("native graph cardinality reply type"),
        })
        .map_err(error)
    }

    fn change_graph(&self, py: Python<'_>, source: String, remove: bool) -> PyResult<()> {
        py.detach(|| {
            self.session
                .execute(Operation::Graph(source, remove))
                .map(|_| ())
        })
        .map_err(error)
    }
    fn commit(&self, py: Python<'_>) -> PyResult<()> {
        py.detach(|| self.session.finish(true)).map_err(error)
    }

    fn rollback(&self, py: Python<'_>) -> PyResult<()> {
        py.detach(|| self.session.finish(false)).map_err(error)
    }

    fn __len__(&self, py: Python<'_>) -> PyResult<usize> {
        py.detach(|| match self.session.execute(Operation::Len)? {
            Reply::Count(value) => Ok(value),
            _ => unreachable!("native transaction reply type"),
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
        let format = match format(rdf_format) {
            Ok(format) => format,
            Err(cause) => {
                py.detach(|| self.session.finish(false)).map_err(error)?;
                return Err(cause);
            }
        };
        py.detach(|| {
            self.session
                .execute(Operation::Load {
                    source,
                    format,
                    base,
                })
                .map(|_| ())
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
            self.session
                .execute(Operation::Infer(
                    mode,
                    destination,
                    crate::inference::InferenceOptions {
                        axiomatic_triples,
                        datatype_axioms,
                    },
                ))
                .map(|_| ())
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
            self.session
                .execute(Operation::InferGraph {
                    mode,
                    source: source_graph,
                    destination: destination_graph,
                })
                .map(|_| ())
        })
        .map_err(error)
    }

    fn update(&self, py: Python<'_>, source: String) -> PyResult<()> {
        py.detach(|| self.session.execute(Operation::Update(source)).map(|_| ()))
            .map_err(error)
    }

    #[pyo3(signature = (source, default_union=false))]
    fn query(
        &self,
        py: Python<'_>,
        source: String,
        default_union: bool,
    ) -> PyResult<(String, String)> {
        py.detach(|| {
            match self
                .session
                .execute(Operation::Query(source, default_union))?
            {
                Reply::Query(kind, text) => Ok((kind, text)),
                _ => unreachable!("native transaction reply type"),
            }
        })
        .map_err(error)
    }

    fn dump(&self, py: Python<'_>, rdf_format: &str) -> PyResult<String> {
        let format = match format(rdf_format) {
            Ok(format) => format,
            Err(cause) => {
                py.detach(|| self.session.finish(false)).map_err(error)?;
                return Err(cause);
            }
        };
        py.detach(|| match self.session.execute(Operation::Dump(format))? {
            Reply::Text(text) => Ok(text),
            _ => unreachable!("native transaction reply type"),
        })
        .map_err(error)
    }

    fn remove_pattern(&self, py: Python<'_>, source: String) -> PyResult<()> {
        py.detach(|| self.session.execute(Operation::Remove(source)).map(|_| ()))
            .map_err(error)
    }

    fn match_quads(&self, py: Python<'_>, source: String) -> PyResult<String> {
        py.detach(|| match self.session.execute(Operation::Match(source))? {
            Reply::Text(text) => Ok(text),
            _ => unreachable!("native quad match reply type"),
        })
        .map_err(error)
    }

    fn query_graph(
        &self,
        py: Python<'_>,
        source: String,
        graph: String,
    ) -> PyResult<(String, String)> {
        py.detach(
            || match self.session.execute(Operation::QueryGraph(source, graph))? {
                Reply::Query(kind, text) => Ok((kind, text)),
                _ => unreachable!("native graph query reply type"),
            },
        )
        .map_err(error)
    }
}
