//! Shape-local focus owners, independent of Boolean-value admission.
use super::{caller::Caller, live};
use pyo3::{
    prelude::*,
    types::{PyList, PySet},
};
use pyshacl_turbo_engine::{
    Error, ReportTermRole, Result, SelectedFocusBatch, SelectedFocusFrame, SelectedFocusSource,
    SelectedLiteral, SelectedTargetsRef, SelectedValueFrame, SelectedValueSource,
    oxigraph::model::*,
};
use std::sync::{Arc, Mutex, Weak};

pub(super) struct Payload {
    pub _set: Py<PySet>,
    pub selected: Py<PyList>,
    pub words: Arc<Vec<Term>>,
}
pub(super) struct FocusFrame {
    pub caller: Weak<Caller>,
    values: Arc<dyn SelectedValueFrame>,
    payload: Mutex<Option<Payload>>,
}
impl std::fmt::Debug for FocusFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ActiveSourceFocusBatch")
    }
}
impl FocusFrame {
    pub(super) fn caller(&self) -> Result<Arc<Caller>> {
        let caller = self
            .caller
            .upgrade()
            .ok_or_else(|| Error::Invalid("Selected caller has ended".into()))?;
        caller.check_thread()?;
        Ok(caller)
    }
    pub(super) fn object(&self, py: Python<'_>, index: usize) -> PyResult<Py<PyAny>> {
        let payload = self.payload.lock().expect("source focus lock poisoned");
        let payload = payload
            .as_ref()
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("Focus batch has ended"))?;
        Ok(payload.selected.bind(py).get_item(index)?.unbind())
    }
}
impl SelectedFocusSource for Caller {
    fn acquire_focus(
        &self,
        targets: SelectedTargetsRef<'_>,
        filter: Option<&[Term]>,
    ) -> Result<SelectedFocusBatch> {
        if !targets.is_core() {
            return Err(Error::Invalid(
                "Source Core focus request contains advanced targets".into(),
            ));
        }
        let payload = self.event(|py| super::focus_acquire::acquire(py, self, targets, filter))?;
        let words = payload.words.clone();
        let values = self.begin_frame()?;
        let frame = Arc::new(FocusFrame {
            caller: self.own.clone(),
            values,
            payload: Mutex::new(Some(payload)),
        });
        Ok(SelectedFocusBatch::new(frame, words))
    }
}
impl SelectedFocusFrame for FocusFrame {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn values(&self) -> &Arc<dyn SelectedValueFrame> {
        &self.values
    }
    fn begin_nested(&self) -> Result<Arc<dyn SelectedValueFrame>> {
        self.caller()?.begin_frame()
    }
    fn datatype(&self, index: usize, expected: &NamedNode) -> Result<bool> {
        let caller = self.caller()?;
        caller.event(|py| {
            let object = self.object(py, index)?;
            super::assessment::datatype(py, &caller, object.bind(py), expected)
        })
    }
    fn retain_literal(&self, index: usize) -> Result<Option<SelectedLiteral>> {
        let caller = self.caller()?;
        Python::attach(|py| {
            caller
                .check_interpreter(py)
                .map_err(|e| Error::Invalid(e.to_string()))?;
            let payload = self.payload.lock().expect("source focus lock poisoned");
            let payload = payload
                .as_ref()
                .ok_or_else(|| Error::Invalid("Focus batch has ended".into()))?;
            let word = payload
                .words
                .get(index)
                .ok_or_else(|| Error::Invalid("Focus slot was not issued".into()))?;
            let Term::Literal(word) = word else {
                return Ok(None);
            };
            let literal = payload
                .selected
                .bind(py)
                .get_item(index)
                .map_err(|e| Error::Invalid(e.to_string()))?
                .unbind();
            let mut frames = caller.frames.lock().expect("source frames lock poisoned");
            let receipt = frames
                .next_receipt
                .checked_add(1)
                .ok_or_else(|| Error::Invalid("Selected receipt identity exhausted".into()))?;
            frames.next_receipt = receipt;
            Ok(Some(SelectedLiteral(Arc::new(live::FocusOwner {
                literal,
                issuer: caller.issuer.clone(),
                word: word.clone(),
                receipt,
            }))))
        })
    }
    fn present(&self, index: usize, _role: ReportTermRole) -> Result<String> {
        let caller = self.caller()?;
        caller.event(|py| {
            let object = self.object(py, index)?;
            let text = py
                .import("rdflib_turbo._literal")?
                .getattr("_selected_presentation")?
                .call1((caller.graph.bind(py), object))?;
            Ok(text.cast::<pyo3::types::PyString>()?.to_str()?.to_owned())
        })
    }
    fn close(&self) {
        if let Some(caller) = self.caller.upgrade() {
            caller
                .check_thread()
                .expect("Focus batch closed on foreign caller thread");
            Python::attach(|py| {
                caller
                    .check_interpreter(py)
                    .expect("Focus batch closed in foreign interpreter");
                let removed = self
                    .payload
                    .lock()
                    .expect("source focus lock poisoned")
                    .take();
                drop(removed);
                self.values.close();
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_focus_layouts_are_measured_without_per_value_holder_growth() {
        println!(
            "FOCUS_NEW_LAYOUT Frame={} Payload={} Batch={} Slot={} Subject={} LiteralOwner={} Observation={} Caller={} Acquired={}",
            std::mem::size_of::<FocusFrame>(),
            std::mem::size_of::<Payload>(),
            std::mem::size_of::<SelectedFocusBatch>(),
            std::mem::size_of::<pyshacl_turbo_engine::SelectedFocusSlot>(),
            std::mem::size_of::<pyshacl_turbo_engine::SelectedSubjectRef<'_>>(),
            std::mem::size_of::<live::FocusOwner>(),
            std::mem::size_of::<pyshacl_turbo_engine::BooleanObservation>(),
            std::mem::size_of::<Caller>(),
            std::mem::size_of::<live::Acquired>()
        );
        assert_eq!(std::mem::size_of::<SelectedFocusBatch>(), 24);
        assert_eq!(
            std::mem::size_of::<pyshacl_turbo_engine::SelectedFocusSlot>(),
            24
        );
        assert_eq!(
            std::mem::size_of::<pyshacl_turbo_engine::BooleanObservation>(),
            32
        );
        assert_eq!(std::mem::size_of::<live::Acquired>(), 24);
    }
}
