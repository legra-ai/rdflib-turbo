//! Keep validation and rule writes on the existing transaction owner.
use super::operations::{Operation, Reply};
use super::session::Session;
use super::shacl_read::CommittedGraph;
use super::{NativeStore, NativeTransaction};
use crate::Result;
use crate::shacl::Report;
use oxigraph::model::{Dataset, GraphName, QuadRef};
use oxigraph::store::Transaction;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyshacl_turbo_engine::{CompiledShapes, Options};
use std::sync::Arc;

/// Derived-only output retains its logical scope until final publication.
pub(super) struct ValidationDelta {
    pub data: Dataset,
    pub graph: Option<GraphName>,
}

impl ValidationDelta {
    pub fn apply(&self, transaction: &mut Transaction<'_>) {
        for quad in &self.data {
            transaction.insert(QuadRef {
                graph_name: self
                    .graph
                    .as_ref()
                    .map_or(quad.graph_name, GraphName::as_ref),
                ..quad
            });
        }
    }
}

impl Reply {
    fn into_validation(self) -> Report {
        match self {
            Self::Validation(report) => *report,
            _ => unreachable!("native validation reply type"),
        }
    }
}

impl NativeStore {
    pub(crate) fn validate_shapes(
        &self,
        shapes: Arc<CompiledShapes>,
        options: Options,
        inplace: bool,
        graph: Option<GraphName>,
    ) -> Result<Report> {
        if inplace {
            return Session::execute_inline(
                self.control.clone(),
                Operation::Validate {
                    shapes,
                    options,
                    inplace,
                    graph,
                },
            )
            .map(Reply::into_validation);
        }
        let store = self.snapshot()?;
        let source = CommittedGraph(store.read_snapshot());
        let report = if let Some(graph) = graph {
            shapes.validate_staged(
                &pyshacl_turbo_engine::ScopedGraph::new(&source, graph),
                options,
            )?
        } else {
            shapes.validate_staged(&source, options)?
        };
        Report::try_new(report)
    }
}

impl NativeTransaction {
    pub(crate) fn validate_shapes(
        &self,
        shapes: Arc<CompiledShapes>,
        options: Options,
        inplace: bool,
        graph: Option<GraphName>,
    ) -> Result<Report> {
        let (report, revision) = self.prepare_shapes(shapes, options, inplace, graph)?;
        self.session.accept_validation(revision)?;
        Ok(report)
    }

    pub(crate) fn prepare_shapes(
        &self,
        shapes: Arc<CompiledShapes>,
        options: Options,
        inplace: bool,
        graph: Option<GraphName>,
    ) -> Result<(Report, u64)> {
        match self.session.execute(Operation::Validate {
            shapes,
            options,
            inplace,
            graph,
        })? {
            Reply::PreparedValidation(prepared) => Ok(*prepared),
            _ => unreachable!("native prepared validation reply type"),
        }
    }

    pub(crate) fn prepare_validation(
        &self,
        shapes: Arc<CompiledShapes>,
        options: Options,
        inplace: bool,
        graph: Option<GraphName>,
    ) -> Result<PreparedValidation> {
        let (report, revision) = self.prepare_shapes(shapes, options, inplace, graph)?;
        Ok(PreparedValidation {
            session: self.session.clone(),
            report: Some(report),
            revision: Some(revision),
        })
    }

    pub(crate) fn abort_validation(&self) -> Result<()> {
        self.session.finish(false)
    }
}

/// Owns private rule output until Python has accepted warnings and decoded the report.
#[pyclass(module = "rdflib_turbo._native")]
pub(crate) struct PreparedValidation {
    session: Arc<Session>,
    report: Option<Report>,
    revision: Option<u64>,
}

#[pymethods]
impl PreparedValidation {
    fn output(&mut self) -> PyResult<crate::shacl::Output> {
        self.report
            .take()
            .map(Report::into_output)
            .ok_or_else(|| PyRuntimeError::new_err("Validation output was already consumed"))
    }

    fn accept(&mut self, py: Python<'_>) -> PyResult<()> {
        if self.report.is_some() {
            return Err(PyRuntimeError::new_err(
                "Validation output must be consumed before acceptance",
            ));
        }
        let revision = self
            .revision
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Validation output was already accepted"))?;
        py.detach(|| self.session.accept_validation(revision))
            .map_err(crate::error)
    }
}

impl Drop for PreparedValidation {
    fn drop(&mut self) {
        if self.revision.is_some() {
            // Disconnect/rollback must also cover a discarded or partially decoded result.
            let _ = self.session.finish(false);
        }
    }
}
