//! Keep validation and rule writes on the existing transaction owner.
use super::operations::{Operation, Reply};
use super::session::Session;
use super::shacl_read::CommittedGraph;
use super::{NativeStore, NativeTransaction};
use crate::Result;
use crate::shacl::Report;
use pyshacl_turbo_engine::{CompiledShapes, Options};
use std::sync::Arc;

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
    ) -> Result<Report> {
        if inplace {
            return Session::execute_inline(
                self.control.clone(),
                Operation::Validate {
                    shapes,
                    options,
                    inplace,
                },
            )
            .map(Reply::into_validation);
        }
        let store = self.snapshot()?;
        let source = CommittedGraph(store.read_snapshot());
        Report::try_new(shapes.validate_staged(&source, options)?)
    }
}

impl NativeTransaction {
    pub(crate) fn validate_shapes(
        &self,
        shapes: Arc<CompiledShapes>,
        options: Options,
        inplace: bool,
    ) -> Result<Report> {
        self.session
            .execute(Operation::Validate {
                shapes,
                options,
                inplace,
            })
            .map(Reply::into_validation)
    }

    pub(crate) fn abort_validation(&self) -> Result<()> {
        self.session.finish(false)
    }
}
