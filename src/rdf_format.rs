//! RDF syntax names accepted at the Python boundary.
use oxigraph::io::RdfFormat;
use pyo3::exceptions::PyNotImplementedError;
use pyo3::prelude::*;

pub(crate) fn format(name: &str) -> PyResult<RdfFormat> {
    match name {
        "turtle" | "ttl" => Ok(RdfFormat::Turtle),
        "nt" | "ntriples" => Ok(RdfFormat::NTriples),
        "nquads" | "nq" => Ok(RdfFormat::NQuads),
        "trig" => Ok(RdfFormat::TriG),
        "xml" | "rdfxml" => Ok(RdfFormat::RdfXml),
        "n3" => Ok(RdfFormat::N3),
        "json-ld" | "jsonld" => Ok(RdfFormat::JsonLd {
            profile: Default::default(),
        }),
        _ => Err(PyNotImplementedError::new_err(format!(
            "Unsupported RDF format: {name}"
        ))),
    }
}
