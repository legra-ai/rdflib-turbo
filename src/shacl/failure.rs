//! The SHACL engine's errors as Python exceptions.
use pyo3::exceptions::{PyNotImplementedError, PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyshacl_turbo_engine::Error;

pyo3::create_exception!(_native, SharedValidationFailure, PyValueError);

pub(crate) fn error(error: Error) -> PyErr {
    match error {
        Error::Comparison(failure) => {
            Python::attach(|py| shacl_bindings::comparison_error(py, failure))
        }
        Error::ValidationFailure(_) => SharedValidationFailure::new_err(error.to_string()),
        Error::ShapeLoad { message, link } => {
            shacl_bindings::ShapeLoadFailure::new_err((message, link))
        }
        Error::ConstraintLoad { message, link } => {
            shacl_bindings::ConstraintLoadFailure::new_err((message, link))
        }
        Error::TypeError(message) => PyTypeError::new_err(message),
        Error::Unsupported(_) => PyNotImplementedError::new_err(error.to_string()),
        Error::Invalid(_) => PyValueError::new_err(error.to_string()),
        _ => PyRuntimeError::new_err(error.to_string()),
    }
}
