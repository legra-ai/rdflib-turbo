//! Original-thread source events; detached native work never dereferences Python.
use super::{Acquisition, BooleanInput, frame, live};
use crate::{SourceComparisonInvocation, SourceComparisonPublisher};
use pyo3::prelude::*;
use pyo3::types::PyType;
use pyshacl_turbo_engine::oxigraph::model::Literal;
use pyshacl_turbo_engine::{
    ComparisonFailure, ComparisonOperator, ComparisonRole, CoreComparisonPolicy, Error, Result,
    SelectedValueFrame, SelectedValueSource, ValueComparisonPolicy,
};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::{self, ThreadId};

pyo3::create_exception!(
    _native,
    SelectedSourceInterruption,
    pyo3::exceptions::PyException
);

#[derive(Debug)]
pub(super) struct CurrentPolicy(RwLock<Arc<dyn ValueComparisonPolicy>>);
impl ValueComparisonPolicy for CurrentPolicy {
    fn compare_numeric(
        &self,
        role: ComparisonRole,
        op: ComparisonOperator,
        left: &Literal,
        right: &Literal,
    ) -> std::result::Result<bool, ComparisonFailure> {
        self.0
            .read()
            .expect("source policy lock poisoned")
            .compare_numeric(role, op, left, right)
    }
}
pub(super) struct ActiveFrame {
    pub id: usize,
    pub issuer: Arc<()>,
    pub owners: Vec<live::Acquired>,
}
pub(super) struct Frames {
    pub next_frame: usize,
    pub next_receipt: usize,
    pub active: Vec<ActiveFrame>,
}
pub(crate) struct Caller {
    pub graph: Py<PyAny>,
    pub classes: [Py<PyType>; 3],
    pub acquisition: Acquisition,
    pub values_admitted: bool,
    pub issuer: Arc<live::Issuer>,
    pub(super) frames: Mutex<Frames>,
    thread: ThreadId,
    publisher: Arc<dyn SourceComparisonPublisher>,
    policy: Arc<CurrentPolicy>,
    failure: Mutex<Option<PyErr>>,
    pub(super) own: std::sync::Weak<Caller>,
}
impl std::fmt::Debug for Caller {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OrdinarySelectedCaller")
    }
}
impl Caller {
    pub fn new(
        py: Python<'_>,
        input: &BooleanInput,
        comparison: SourceComparisonInvocation,
    ) -> PyResult<Arc<Self>> {
        let issuer = Arc::new(live::Issuer {
            interpreter: crate::comparison::interpreter(py)?,
        });
        let SourceComparisonInvocation { policy, publisher } = comparison;
        let policy = Arc::new(CurrentPolicy(RwLock::new(policy)));
        Ok(Arc::new_cyclic(|own| Self {
            graph: input.graph.clone_ref(py),
            classes: input.classes.each_ref().map(|c| c.clone_ref(py)),
            acquisition: input.acquisition,
            values_admitted: input.canonical_booleans,
            issuer,
            frames: Mutex::new(Frames {
                next_frame: 0,
                next_receipt: 0,
                active: Vec::new(),
            }),
            thread: thread::current().id(),
            publisher,
            policy,
            failure: Mutex::new(None),
            own: own.clone(),
        }))
    }
    pub fn authority(&self) -> CoreComparisonPolicy {
        CoreComparisonPolicy::Host(self.policy.clone())
    }
    pub fn event<T>(&self, operation: impl FnOnce(Python<'_>) -> PyResult<T>) -> Result<T> {
        self.check_thread()?;
        if self
            .failure
            .lock()
            .expect("source failure lock poisoned")
            .is_some()
        {
            return Err(Error::SelectedSourceInterrupted);
        }
        Python::attach(|py| {
            let result = (|| {
                self.check_interpreter(py)?;
                self.publisher.publish_status(py)?;
                let value = operation(py)?;
                let renewed = self.publisher.refresh_after_source(py)?;
                *self.policy.0.write().expect("source policy lock poisoned") = renewed;
                Ok(value)
            })();
            result.map_err(|error| {
                let mut first = self.failure.lock().expect("source failure lock poisoned");
                if first.is_none() {
                    *first = Some(error);
                }
                Error::SelectedSourceInterrupted
            })
        })
    }
    pub fn check_interpreter(&self, py: Python<'_>) -> PyResult<()> {
        if crate::comparison::interpreter(py)? != self.issuer.interpreter {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Selected source belongs to another interpreter",
            ));
        }
        Ok(())
    }
    pub fn check_thread(&self) -> Result<()> {
        if thread::current().id() != self.thread {
            return Err(Error::Invalid(
                "Selected source requires its original caller thread".into(),
            ));
        }
        Ok(())
    }
    pub fn finish<T>(&self, py: Python<'_>, result: Result<T>) -> PyResult<T> {
        let error = self
            .failure
            .lock()
            .expect("source failure lock poisoned")
            .take();
        if let Some(error) = error {
            return Err(SelectedSourceInterruption::new_err((
                error.value(py).clone().unbind(),
                error.traceback(py).map(Bound::unbind),
            )));
        }
        self.publisher.publish(py)?;
        result.map_err(crate::error)
    }
    pub fn close(&self, id: usize, issuer: &Arc<()>) {
        self.check_thread()
            .expect("Selected frame closed on a foreign caller thread");
        Python::attach(|py| {
            self.check_interpreter(py)
                .expect("Selected frame closed in a foreign interpreter");
            let removed = {
                let mut frames = self.frames.lock().expect("source frames lock poisoned");
                frames
                    .active
                    .iter()
                    .position(|f| f.id == id && Arc::ptr_eq(&f.issuer, issuer))
                    .map(|i| frames.active.remove(i))
            };
            // Release acquired Python objects attached and outside the frame mutex.
            drop(removed);
        });
    }
}
impl SelectedValueSource for Caller {
    fn begin_frame(&self) -> Result<Arc<dyn SelectedValueFrame>> {
        self.check_thread()?;
        if self
            .failure
            .lock()
            .expect("source failure lock poisoned")
            .is_some()
        {
            return Err(Error::SelectedSourceInterrupted);
        }
        let mut frames = self.frames.lock().expect("source frames lock poisoned");
        let id = frames
            .next_frame
            .checked_add(1)
            .ok_or_else(|| Error::Invalid("Selected frame identity exhausted".into()))?;
        frames.next_frame = id;
        let issuer = Arc::new(());
        frames.active.push(ActiveFrame {
            id,
            issuer: issuer.clone(),
            owners: Vec::new(),
        });
        Ok(Arc::new(frame::Frame {
            caller: self.own.clone(),
            id,
            issuer,
        }))
    }
}
