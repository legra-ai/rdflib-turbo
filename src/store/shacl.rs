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
    pub inference: Dataset,
    pub graph: Option<GraphName>,
}

impl ValidationDelta {
    pub fn apply(&self, transaction: &mut Transaction<'_>) {
        for quad in self.data.iter().chain(self.inference.iter()) {
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
    pub(crate) fn validate_inferred_readonly(
        &self,
        shapes: &CompiledShapes,
        options: Options,
        graph: Option<GraphName>,
        inference: crate::shacl::InferenceStage,
    ) -> Result<Report> {
        let store = self.snapshot()?;
        let source = CommittedGraph(store.read_snapshot());
        super::shacl_inference::validate(&source, shapes, options, false, graph, inference)
            .map(|(report, _)| report)
    }

    pub(crate) fn prepare_inferred_validation(
        &self,
        shapes: Arc<CompiledShapes>,
        options: Options,
        graph: Option<GraphName>,
        inference: crate::shacl::InferenceStage,
    ) -> Result<PreparedValidation> {
        let transaction = NativeTransaction {
            session: Arc::new(Session::start(self.control.clone())?),
        };
        let mut prepared =
            transaction.prepare_inferred_validation(shapes, options, true, graph, inference)?;
        prepared.commit_on_accept = true;
        Ok(prepared)
    }

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
    pub(crate) fn prepare_inferred_validation(
        &self,
        shapes: Arc<CompiledShapes>,
        options: Options,
        inplace: bool,
        graph: Option<GraphName>,
        inference: crate::shacl::InferenceStage,
    ) -> Result<PreparedValidation> {
        match self.session.execute(Operation::ValidateInferred {
            shapes,
            options,
            inplace,
            graph,
            inference: Box::new(inference),
        })? {
            Reply::PreparedValidation(prepared) => {
                let (report, revision) = *prepared;
                let diagnostics = report.has_diagnostics();
                Ok(PreparedValidation {
                    session: self.session.clone(),
                    report: Some(report),
                    revision: Some(revision),
                    commit_on_accept: false,
                    diagnostics,
                })
            }
            _ => unreachable!("native prepared validation reply type"),
        }
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
            commit_on_accept: false,
            diagnostics: false,
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
    commit_on_accept: bool,
    diagnostics: bool,
}

#[pymethods]
impl PreparedValidation {
    fn has_diagnostics(&self) -> bool {
        self.diagnostics
    }
    pub(crate) fn output(&mut self) -> PyResult<crate::shacl::Output> {
        self.report
            .take()
            .map(Report::into_output)
            .ok_or_else(|| PyRuntimeError::new_err("Validation output was already consumed"))
    }

    pub(crate) fn accept(&mut self, py: Python<'_>) -> PyResult<()> {
        if self.report.is_some() {
            return Err(PyRuntimeError::new_err(
                "Validation output must be consumed before acceptance",
            ));
        }
        let revision = self
            .revision
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Validation output was already accepted"))?;
        py.detach(|| {
            self.session.accept_validation(revision)?;
            if self.commit_on_accept {
                self.session.finish(true)?;
            }
            Ok(())
        })
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
