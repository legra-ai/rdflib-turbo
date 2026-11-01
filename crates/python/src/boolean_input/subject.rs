//! Checked inherited ownership; no RDF equality or lexical recovery of a lease.
use super::{caller::Caller, focus::FocusFrame, frame::Frame};
use pyo3::prelude::*;
use pyshacl_turbo_engine::{
    Error, Result, SelectedFocusFrame, SelectedSubjectRef, SelectedValueFrame,
};
use std::sync::Arc;

pub(super) enum Issued {
    Value(Arc<dyn SelectedValueFrame>, usize),
    Focus(Arc<dyn SelectedFocusFrame>, usize),
}
impl Issued {
    pub fn resolve(
        caller: &Arc<Caller>,
        subject: Option<SelectedSubjectRef<'_>>,
    ) -> Result<Option<Self>> {
        let resolved = match subject {
            None => return Ok(None),
            Some(SelectedSubjectRef::Value(slot)) => {
                let (issuer, index) = slot.issued()?;
                let frame = issuer
                    .as_any()
                    .downcast_ref::<Frame>()
                    .ok_or_else(|| Error::Invalid("Foreign selected subject issuer".into()))?;
                if !Arc::ptr_eq(caller, &frame.caller()?) {
                    return Err(Error::Invalid(
                        "Selected subject belongs to another caller".into(),
                    ));
                }
                Self::Value(issuer, index)
            }
            Some(SelectedSubjectRef::Focus(slot)) => {
                let (issuer, index) = slot.issued()?;
                let frame = issuer
                    .as_any()
                    .downcast_ref::<FocusFrame>()
                    .ok_or_else(|| Error::Invalid("Foreign selected focus issuer".into()))?;
                if !Arc::ptr_eq(caller, &frame.caller()?) {
                    return Err(Error::Invalid(
                        "Selected focus belongs to another caller".into(),
                    ));
                }
                Self::Focus(issuer, index)
            }
        };
        Ok(Some(resolved))
    }
    pub fn object(&self, py: Python<'_>, caller: &Caller) -> PyResult<Py<PyAny>> {
        match self {
            Self::Value(issuer, index) => issuer
                .as_any()
                .downcast_ref::<Frame>()
                .expect("checked value frame")
                .object(py, caller, *index),
            Self::Focus(issuer, index) => issuer
                .as_any()
                .downcast_ref::<FocusFrame>()
                .expect("checked focus frame")
                .object(py, *index),
        }
    }
}
