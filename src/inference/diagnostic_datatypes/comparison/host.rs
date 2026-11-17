//! The Python comparison host that captures per-invocation policy and signals.
use std::sync::Arc;

use pyo3::prelude::*;
use shacl_bindings::{ComparisonHost, ComparisonInvocation, ComparisonPublisher};

use super::super::{LexicalContext, SignalPublisher, comparison_warning};
use super::policy::Policy;
use super::source;

pub(crate) struct Host {
    site: comparison_warning::Site,
}
impl Host {
    pub(crate) fn new(py: Python<'_>) -> PyResult<Self> {
        Ok(Self {
            site: comparison_warning::Site::read(py)?,
        })
    }
}

impl ComparisonHost for Host {
    fn capture_source(
        &self,
        py: Python<'_>,
    ) -> PyResult<shacl_bindings::SourceComparisonInvocation> {
        source::capture(py, &self.site)
    }
    fn capture(&self, py: Python<'_>) -> PyResult<ComparisonInvocation> {
        let (context, signals) = LexicalContext::capture_with_publisher(py)?;
        let (warning, warnings) = comparison_warning::capture(py, &self.site)?;
        Ok(ComparisonInvocation {
            policy: Arc::new(Policy { context, warning }),
            publisher: Box::new(Publisher { signals, warnings }),
        })
    }
}
struct Publisher {
    signals: SignalPublisher,
    warnings: comparison_warning::Publisher,
}
impl ComparisonPublisher for Publisher {
    fn publish(&self, py: Python<'_>) -> PyResult<()> {
        self.signals.publish(py)?;
        self.warnings.publish(py)
    }
}
