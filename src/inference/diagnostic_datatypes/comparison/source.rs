//! Refreshable source-call checkpoints, without changing ordinary publisher layout.
use super::{LexicalContext, Policy, SignalPublisher, comparison_warning};
use pyo3::{exceptions::PyRuntimeError, prelude::*};
use pyshacl_turbo_engine::ValueComparisonPolicy;
use shacl_bindings::{ComparisonPublisher, SourceComparisonInvocation, SourceComparisonPublisher};
use std::sync::{Arc, Mutex};

pub(super) fn capture(
    py: Python<'_>,
    site: &comparison_warning::Site,
) -> PyResult<SourceComparisonInvocation> {
    let (context, signals) = LexicalContext::capture_with_publisher(py)?;
    let (warning, warnings) = comparison_warning::capture(py, site)?;
    Ok(SourceComparisonInvocation {
        policy: Arc::new(Policy { context, warning }),
        publisher: Arc::new(Publisher {
            signals: Mutex::new(Some(signals)),
            warnings,
        }),
    })
}

struct Publisher {
    signals: Mutex<Option<SignalPublisher>>,
    warnings: comparison_warning::Publisher,
}
impl Publisher {
    fn signals<T>(&self, operation: impl FnOnce(&SignalPublisher) -> PyResult<T>) -> PyResult<T> {
        let signals = self
            .signals
            .lock()
            .expect("source comparison lock poisoned")
            .take()
            .ok_or_else(|| {
                PyRuntimeError::new_err("Source comparison publisher is already active")
            })?;
        // Never retain even this private status mutex while calling Python mappings.
        let result = operation(&signals);
        *self
            .signals
            .lock()
            .expect("source comparison lock poisoned") = Some(signals);
        result
    }
}
impl ComparisonPublisher for Publisher {
    fn publish(&self, py: Python<'_>) -> PyResult<()> {
        self.signals(|signals| signals.publish(py))?;
        self.warnings.publish(py)
    }
}
impl SourceComparisonPublisher for Publisher {
    fn publish_status(&self, py: Python<'_>) -> PyResult<()> {
        self.signals(|signals| signals.checkpoint(py))
    }
    fn refresh_after_source(&self, py: Python<'_>) -> PyResult<Arc<dyn ValueComparisonPolicy>> {
        let (context, signals) = LexicalContext::capture_with_publisher(py)?;
        let warning = self.warnings.refresh_profile(py)?;
        let previous = self
            .signals
            .lock()
            .expect("source comparison lock poisoned")
            .replace(signals);
        // Old Python references are released outside the mutex too.
        drop(previous);
        Ok(Arc::new(Policy { context, warning }))
    }
}
