//! Only actual SH.value occurrences promote acquired objects into retained owners.
use pyo3::prelude::*;
use pyshacl_turbo_engine::SelectedLiteralOwner;
use std::{any::Any, sync::Arc};

pub(super) struct Acquired {
    pub literal: Py<PyAny>,
    pub word: bool,
    pub receipt: usize,
}
pub(crate) struct Issuer {
    pub interpreter: i64,
}

pub(crate) struct Owner {
    pub literal: Py<PyAny>,
    pub issuer: Arc<Issuer>,
    pub word: bool,
    pub receipt: usize,
}
impl std::fmt::Debug for Owner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SelectedBooleanReportOwner")
    }
}
impl SelectedLiteralOwner for Owner {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub(crate) struct FocusOwner {
    pub literal: Py<PyAny>,
    pub issuer: Arc<Issuer>,
    pub word: pyshacl_turbo_engine::oxigraph::model::Literal,
    pub receipt: usize,
}
impl std::fmt::Debug for FocusOwner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SelectedLiteralFocusReportOwner")
    }
}
impl SelectedLiteralOwner for FocusOwner {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Reported<'a> {
    Boolean(&'a Owner),
    Focus(&'a FocusOwner),
}
impl<'a> Reported<'a> {
    pub fn checked(
        owner: &'a pyshacl_turbo_engine::SelectedLiteral,
    ) -> pyshacl_turbo_engine::Result<Self> {
        if let Some(owner) = owner.0.as_any().downcast_ref::<Owner>() {
            return Ok(Self::Boolean(owner));
        }
        if let Some(owner) = owner.0.as_any().downcast_ref::<FocusOwner>() {
            return Ok(Self::Focus(owner));
        }
        Err(pyshacl_turbo_engine::Error::Invalid(
            "Foreign selected report owner".into(),
        ))
    }
    pub fn literal(self) -> &'a Py<PyAny> {
        match self {
            Self::Boolean(o) => &o.literal,
            Self::Focus(o) => &o.literal,
        }
    }
    pub fn issuer(self) -> &'a Arc<Issuer> {
        match self {
            Self::Boolean(o) => &o.issuer,
            Self::Focus(o) => &o.issuer,
        }
    }
    pub fn receipt(self) -> usize {
        match self {
            Self::Boolean(o) => o.receipt,
            Self::Focus(o) => o.receipt,
        }
    }
    pub fn matches(
        self,
        kind: u8,
        text: &str,
        datatype: Option<&str>,
        language: Option<&str>,
    ) -> bool {
        if kind != 2 {
            return false;
        }
        match self {
            Self::Boolean(o) => {
                text == if o.word { "true" } else { "false" }
                    && datatype == Some(super::source::BOOLEAN)
                    && language.is_none()
            }
            Self::Focus(o) => {
                text == o.word.value()
                    && language == o.word.language()
                    && datatype
                        == (o.word.language().is_none() && o.word.has_explicit_datatype())
                            .then_some(o.word.datatype().as_str())
            }
        }
    }
}
