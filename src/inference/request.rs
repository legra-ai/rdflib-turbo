//! Caller-thread conversion configuration and caller-owned diagnostic history.

use pyo3::prelude::*;

use super::InferenceOptions;
use super::diagnostic_datatypes::{LexicalContext, SignalPublisher};

#[derive(Default)]
pub(crate) struct InferenceRequest {
    pub(crate) options: InferenceOptions,
    pub(super) prior_error_messages: Vec<String>,
    pub(super) lexical_context: LexicalContext,
}

impl InferenceRequest {
    pub(crate) fn capture(
        py: Python<'_>,
        prior_error_messages: Option<Vec<String>>,
        options: InferenceOptions,
    ) -> PyResult<(Self, SignalPublisher)> {
        let (lexical_context, publisher) = LexicalContext::capture_with_publisher(py)?;
        Ok((
            Self {
                options,
                prior_error_messages: prior_error_messages.unwrap_or_default(),
                lexical_context,
            },
            publisher,
        ))
    }
}

impl From<InferenceOptions> for InferenceRequest {
    fn from(options: InferenceOptions) -> Self {
        Self {
            options,
            ..Self::default()
        }
    }
}
