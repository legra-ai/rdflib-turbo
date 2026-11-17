//! Python-facing N-Triples documents with rdflib-compatible warnings.

use std::ffi::CString;

use pyo3::IntoPyObjectExt;
use pyo3::exceptions::{PyException, PyUserWarning};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyString};

use super::output;

pub(super) fn nt_python_output(
    py: Python<'_>,
    output: output::GraphNtOutput,
    as_bytes: bool,
) -> PyResult<Py<PyAny>> {
    let output = match output {
        output::GraphNtOutput::WithWarnings(warnings, terminal) => {
            for warning in warnings {
                let value = PyString::new(py, &warning.value).repr()?;
                let datatype = PyString::new(py, &warning.datatype).repr()?;
                let message = format!(
                    "Serializing weird numerical rdflib.term.Literal({}, datatype=rdflib.term.URIRef({}))",
                    value.to_str()?,
                    datatype.to_str()?,
                );
                let message = CString::new(message).expect("Python repr escapes null characters");
                PyErr::warn(py, &py.get_type::<PyUserWarning>(), &message, 1)?;
            }
            *terminal
        }
        terminal => terminal,
    };
    let text = match output {
        output::GraphNtOutput::Document(text) => text,
        output::GraphNtOutput::WithWarnings(..) => {
            unreachable!("Only one warning envelope is emitted")
        }
        output::GraphNtOutput::InvalidUri(uri) => {
            return Err(PyException::new_err(format!(
                "\"{uri}\" does not look like a valid URI, I cannot serialize this as N3/Turtle. Perhaps you wanted to urlencode it?"
            )));
        }
    };
    if as_bytes {
        Ok(PyBytes::new(py, text.as_bytes()).into_any().unbind())
    } else {
        text.into_py_any(py)
    }
}
