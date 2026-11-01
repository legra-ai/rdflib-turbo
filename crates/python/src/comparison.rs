//! An immutable host factory and invocation-local Core comparison authority.
use std::sync::Arc;

use pyo3::exceptions::{PyDeprecationWarning, PyRuntimeError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::PyType;
use pyshacl_turbo_engine::{ComparisonFailure, CoreComparisonPolicy, ValueComparisonPolicy};

/// A caller-thread publisher; native policy evaluation never dereferences it.
pub trait ComparisonPublisher: Send + Sync {
    /// Publish newly raised status after every producer has completed.
    fn publish(&self, py: Python<'_>) -> PyResult<()>;
}
/// An invocation owns its native scalar policy and caller-side finalizer separately.
pub struct ComparisonInvocation {
    /// Pure-native behavior moved into the validation options.
    pub policy: Arc<dyn ValueComparisonPolicy>,
    /// Python output and diagnostic effects, used only after detached work returns.
    pub publisher: Box<dyn ComparisonPublisher>,
}
impl ComparisonInvocation {
    /// Return the explicitly selected native host policy for this invocation.
    pub fn authority(&self) -> CoreComparisonPolicy {
        CoreComparisonPolicy::Host(Arc::clone(&self.policy))
    }
    /// Publish flags and warnings before translating a typed native failure.
    pub fn finish<T>(self, py: Python<'_>, result: pyshacl_turbo_engine::Result<T>) -> PyResult<T> {
        self.publisher.publish(py)?;
        result.map_err(super::error)
    }
}
/// Source checkpoints are distinct from terminal publication.
pub trait SourceComparisonPublisher: ComparisonPublisher {
    /// Drain completed native status before invoking source code.
    fn publish_status(&self, py: Python<'_>) -> PyResult<()>;
    /// Refresh caller policy while preserving pending warning occurrences.
    fn refresh_after_source(&self, py: Python<'_>) -> PyResult<Arc<dyn ValueComparisonPolicy>>;
}
/// Allocated only for an invocation that actually admits selected source events.
pub struct SourceComparisonInvocation {
    /// Initial native policy.
    pub policy: Arc<dyn ValueComparisonPolicy>,
    /// Caller-only checkpoint/finalization authority.
    pub publisher: Arc<dyn SourceComparisonPublisher>,
}
/// Provider-owned factory; capture is permitted only on the attached caller thread.
pub trait ComparisonHost: Send + Sync {
    /// Snapshot the current caller's context, never a compilation-cache context.
    fn capture(&self, py: Python<'_>) -> PyResult<ComparisonInvocation>;
    /// Capture refreshable authority only for explicit ordinary source events.
    fn capture_source(&self, py: Python<'_>) -> PyResult<SourceComparisonInvocation>;
}
/// A sealed interpreter-local factory token. It owns no mutable process-global context.
#[pyclass(frozen, name = "_ValueComparisonPolicy")]
pub struct NativeValueComparisonPolicy {
    host: Arc<dyn ComparisonHost>,
    interpreter: i64,
}
pub(crate) fn interpreter(py: Python<'_>) -> PyResult<i64> {
    // SAFETY: the token proves an attached interpreter; no borrowed pointer escapes.
    let identity =
        unsafe { pyo3::ffi::PyInterpreterState_GetID(pyo3::ffi::PyInterpreterState_Get()) };
    if identity < 0 {
        return Err(PyErr::fetch(py));
    }
    Ok(identity)
}
/// Capture using only the explicit sealed token supplied by the same native module.
pub fn capture_comparison(
    py: Python<'_>,
    policy: Py<NativeValueComparisonPolicy>,
) -> PyResult<ComparisonInvocation> {
    comparison_host(py, policy)?.capture(py)
}
/// Share the immutable factory descriptor without capturing any caller context.
pub fn comparison_host(
    py: Python<'_>,
    policy: Py<NativeValueComparisonPolicy>,
) -> PyResult<Arc<dyn ComparisonHost>> {
    let policy = policy.bind(py).get();
    if interpreter(py)? != policy.interpreter {
        return Err(PyRuntimeError::new_err(
            "Core comparison policy belongs to another interpreter",
        ));
    }
    Ok(Arc::clone(&policy.host))
}

pub(super) fn install(module: &Bound<'_, PyModule>, host: Arc<dyn ComparisonHost>) -> PyResult<()> {
    let py = module.py();
    module.add(
        "_VALUE_COMPARISON_POLICY",
        Py::new(
            py,
            NativeValueComparisonPolicy {
                host,
                interpreter: interpreter(py)?,
            },
        )?,
    )
}
/// Construct exact public exceptions only after native execution and flag publication.
pub fn comparison_error(py: Python<'_>, failure: ComparisonFailure) -> PyErr {
    match failure {
        ComparisonFailure::NotImplementedTruth => {
            PyTypeError::new_err("NotImplemented should not be used in a boolean context")
        }
        ComparisonFailure::DeprecationWarning => {
            PyDeprecationWarning::new_err("NotImplemented should not be used in a boolean context")
        }
        ComparisonFailure::Incomparable { .. } | ComparisonFailure::UnsupportedOrdering { .. } => {
            PyTypeError::new_err(failure.to_string())
        }
        ComparisonFailure::FloatOperation | ComparisonFailure::InvalidOperation => {
            let name = if failure == ComparisonFailure::FloatOperation {
                "FloatOperation"
            } else {
                "InvalidOperation"
            };
            (|| -> PyResult<PyErr> {
                let decimal = py.import("decimal")?;
                let class = decimal.getattr(name)?.cast_into::<PyType>()?;
                let signal = decimal.getattr(name)?;
                Ok(PyErr::from_type(class, (vec![signal.unbind()],)))
            })()
            .unwrap_or_else(|error| error)
        }
    }
}
