//! Serialize the report before its transaction can commit.
use crate::{Error, Result};
use oxigraph::io::{RdfFormat, RdfSerializer};
use pyshacl_turbo_engine::ValidationReport;

pub(crate) struct Report {
    conforms: bool,
    graph: String,
    text: String,
    warnings: Vec<String>,
}

impl Report {
    pub(crate) fn try_new(report: ValidationReport) -> Result<Self> {
        let mut writer = RdfSerializer::from_format(RdfFormat::NTriples).for_writer(Vec::new());
        for quad in report.graph.iter() {
            writer
                .serialize_quad(quad)
                .map_err(|error| Error::Runtime(error.to_string()))?;
        }
        let bytes = writer
            .finish()
            .map_err(|error| Error::Runtime(error.to_string()))?;
        let graph = String::from_utf8(bytes).map_err(|error| Error::Runtime(error.to_string()))?;
        Ok(Self {
            conforms: report.conforms,
            graph,
            text: report.text,
            warnings: report.recursion_warnings,
        })
    }

    pub(crate) fn into_output(self) -> super::Output {
        (self.conforms, self.graph, self.text, self.warnings)
    }
}
