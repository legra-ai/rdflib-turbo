//! The typed Python exception raised when an RDFC-1.0 work budget runs out.
use pyo3::create_exception;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

create_exception!(
    rdflib_turbo._native,
    CanonicalizationBudgetExceeded,
    PyRuntimeError,
    "RDFC-1.0 canonicalization exhausted its caller-set work budget."
);

/// Builds the Python exception after native locks have been released.
pub(crate) fn budget_error(exceeded: oxigraph::model::CanonicalizationBudgetExceeded) -> PyErr {
    CanonicalizationBudgetExceeded::new_err(exceeded.to_string())
}
