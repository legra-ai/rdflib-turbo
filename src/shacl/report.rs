//! Serialize the report before its transaction can commit.
use crate::Result;
use pyshacl_turbo_engine::ValidationReport;

pub(crate) struct Report {
    conforms: bool,
    graph: super::Batch,
    text: String,
    warnings: Vec<String>,
}

impl Report {
    pub(crate) fn try_new(report: ValidationReport) -> Result<Self> {
        let graph = super::ReportTerms::encode(&report.graph);
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
