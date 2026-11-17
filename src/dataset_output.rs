//! Serialized and JSON views of an exact-term dataset.
use oxigraph::io::{RdfFormat, RdfSerializer};
use oxigraph::model::*;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use serde_json::{Value, json};

pub(crate) fn serialize(data: &GeneralizedDataset, format: RdfFormat) -> PyResult<String> {
    let mut writer = RdfSerializer::from_format(format).for_writer(Vec::new());
    for quad in data.iter() {
        let quad =
            QuadRef::try_from(quad).map_err(|error| PyValueError::new_err(error.to_string()))?;
        writer
            .serialize_quad(quad)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
    }
    String::from_utf8(
        writer
            .finish()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?,
    )
    .map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

pub(crate) fn term(term: &Term) -> Value {
    match term {
        Term::NamedNode(value) => json!({"type": "uri", "value": value.as_str()}),
        Term::BlankNode(value) => json!({"type": "bnode", "value": value.as_str()}),
        Term::Literal(value) => {
            let mut result = json!({"type": "literal", "value": value.value()});
            if let Some(language) = value.language() {
                result["xml:lang"] = json!(language);
            } else if value.has_explicit_datatype() {
                result["datatype"] = json!(value.datatype().as_str());
            }
            result
        }
    }
}
