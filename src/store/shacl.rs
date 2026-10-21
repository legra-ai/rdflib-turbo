//! Keep validation and rule writes on the existing transaction owner.
use super::operations::{Operation, Reply};
use super::session::Session;
use super::{NativeStore, NativeTransaction};
use crate::shacl::Report;
use crate::{Error, Result};
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
        let operation = Operation::Validate {
            shapes,
            options,
            inplace,
        };
        if inplace {
            return Session::execute_inline(self.control.clone(), operation)
                .map(Reply::into_validation);
        }
        let store = self.snapshot()?;
        let mut transaction = store
            .start_transaction()
            .map_err(|error| Error::Runtime(error.to_string()))?;
        operation
            .execute(&mut transaction)
            .map(Reply::into_validation)
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
