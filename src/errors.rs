//! Typed native failures; construct Python exceptions after native locks drop.

use pyo3::exceptions::{
    PyAttributeError, PyIndexError, PyNotImplementedError, PyRuntimeError, PyValueError,
};
use pyo3::prelude::*;
use pyo3::types::PyType;

use crate::inference;

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error("Unsupported feature: {0}")]
    Unsupported(String),
    #[error("{0}")]
    Runtime(String),
    #[error("{0}")]
    Attribute(String),
    #[error("Datatype conversion failed: {0:?}")]
    Lexical(inference::LexicalFailure),
    #[error(transparent)]
    Shacl(#[from] pyshacl_turbo_engine::Error),
    #[error("SPARQL evaluation failed: {0}")]
    Query(#[from] oxigraph::sparql::QueryEvaluationError),
}
pub(crate) type Result<T> = std::result::Result<T, Error>;

pub(crate) fn error(error: Error) -> PyErr {
    match error {
        Error::Shacl(cause) => crate::shacl::error(cause),
        Error::Invalid(_) => PyValueError::new_err(error.to_string()),
        Error::Unsupported(_) => PyNotImplementedError::new_err(error.to_string()),
        Error::Attribute(message) => PyAttributeError::new_err(message),
        Error::Lexical(failure) => lexical_error(failure),
        _ => PyRuntimeError::new_err(error.to_string()),
    }
}

fn lexical_error(failure: inference::LexicalFailure) -> PyErr {
    match failure {
        inference::LexicalFailure::IndexError => PyIndexError::new_err("string index out of range"),
        inference::LexicalFailure::DecimalInvalidOperation
        | inference::LexicalFailure::DecimalInvalidComparison => {
            Python::attach(|py| -> PyResult<PyErr> {
                let decimal = py.import("decimal")?;
                let exception = decimal.getattr("InvalidOperation")?.cast_into::<PyType>()?;
                let signal_name = match failure {
                    inference::LexicalFailure::DecimalInvalidOperation => "ConversionSyntax",
                    inference::LexicalFailure::DecimalInvalidComparison => "InvalidOperation",
                    inference::LexicalFailure::IndexError => unreachable!(),
                };
                let signal = decimal.getattr(signal_name)?;
                // bounded: one exact Decimal conversion signal in the reference exception's args.
                Ok(PyErr::from_type(exception, (vec![signal.unbind()],)))
            })
            .unwrap_or_else(|error| error)
        }
    }
}
