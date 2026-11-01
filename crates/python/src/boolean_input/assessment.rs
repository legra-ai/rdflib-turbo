//! Shared deferred datatype consumer; no getter runs at target acquisition.
use super::{caller::Caller, source};
use pyo3::{exceptions::PyAttributeError, prelude::*, types::PyBool};
use pyshacl_turbo_engine::oxigraph::model::NamedNode;
pub(super) fn datatype(
    py: Python<'_>,
    caller: &Caller,
    object: &Bound<'_, PyAny>,
    expected: &NamedNode,
) -> PyResult<bool> {
    let datatype = object.getattr("datatype")?;
    let language = object.getattr("language")?;
    let factories = source::Factories::bind(py, &caller.classes);
    let expected_python = factories.python_term(expected.as_ref().into())?;
    if datatype.eq(&expected_python)? {
        // Original uses getattr(value, "ill_typed", None): only this
        // specific missing-attribute branch has a declared default.
        let invalid = match object.getattr("ill_typed") {
            Ok(assessment) => {
                assessment.is_instance_of::<PyBool>() && assessment.extract::<bool>()?
            }
            Err(error) if error.is_instance_of::<PyAttributeError>(py) => false,
            Err(error) => return Err(error),
        };
        if invalid {
            return Ok(false);
        }
        let value = object.getattr("value")?;
        return Ok(value.is_instance_of::<PyBool>());
    }
    let name = expected.as_str();
    if name == "http://www.w3.org/2000/01/rdf-schema#Literal" {
        return Ok(true);
    }
    if name == "http://www.w3.org/2000/01/rdf-schema#Datatype" && datatype.is_truthy()? {
        return Ok(true);
    }
    let _ = language;
    Ok(false)
}
