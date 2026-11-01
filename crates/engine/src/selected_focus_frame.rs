//! Move-only active focus batch and borrowed subject provenance.
use crate::{
    Error, ReportTermRole, Result, SelectedLiteral, SelectedSlot, SelectedTargetsRef,
    SelectedValueFrame,
};
use oxigraph::model::{NamedNode, Term};
use std::{
    any::Any,
    fmt::Debug,
    sync::{Arc, Weak},
};

/// Explicit source focus acquisition, independent of Boolean value admission.
pub trait SelectedFocusSource: Debug + Send + Sync {
    /// Discover then filter actual source nodes. Empty handled batches remain
    /// distinct from an absent source capability.
    fn acquire_focus(
        &self,
        targets: SelectedTargetsRef<'_>,
        filter: Option<&[Term]>,
    ) -> Result<SelectedFocusBatch>;
}

/// Active source focus owners; no report may retain this whole frame.
pub trait SelectedFocusFrame: Debug + Send + Sync {
    /// Bindings validate the exact source-specific issuer before host access.
    fn as_any(&self) -> &dyn Any;
    /// Borrow this batch's value frame. The focus batch is its sole close owner.
    fn values(&self) -> &Arc<dyn SelectedValueFrame>;
    /// Nested shapes own and close a fresh frame, never the parent batch frame.
    fn begin_nested(&self) -> Result<Arc<dyn SelectedValueFrame>>;
    /// Read an actual selected Literal's assessment only at its consumer.
    fn datatype(&self, index: usize, expected: &NamedNode) -> Result<bool>;
    /// Promote only an actual reported Literal; URI/BNode report cloning is
    /// separate from source-subject acquisition.
    fn retain_literal(&self, index: usize) -> Result<Option<SelectedLiteral>>;
    /// Execute the actual source report formatter for this occurrence.
    fn present(&self, index: usize, role: ReportTermRole) -> Result<String>;
    /// End this focus frame and its value frame, including unwind paths.
    fn close(&self);
}

/// Sole close owner of one source shape batch, never Clone.
pub struct SelectedFocusBatch {
    frame: Arc<dyn SelectedFocusFrame>,
    words: Arc<Vec<Term>>,
}
impl SelectedFocusBatch {
    /// The frame and immutable native words belong to the same acquisition.
    /// The source must preserve each actual Python set slot, even equal words.
    pub fn new(frame: Arc<dyn SelectedFocusFrame>, words: Arc<Vec<Term>>) -> Self {
        Self { frame, words }
    }
    /// Borrow the existing native word allocation; no second F-word array.
    pub fn words(&self) -> &[Term] {
        &self.words
    }
    pub(crate) fn values(&self) -> &Arc<dyn SelectedValueFrame> {
        self.frame.values()
    }
    pub(crate) fn slot(&self, index: usize) -> Result<SelectedFocusSlot> {
        if index >= self.words.len() {
            return Err(Error::Invalid("Focus slot was not issued".into()));
        }
        Ok(SelectedFocusSlot {
            frame: Arc::downgrade(&self.frame),
            index,
        })
    }
}
impl Drop for SelectedFocusBatch {
    fn drop(&mut self) {
        self.frame.close();
    }
}

/// Call-local focus slot. It cannot keep the batch or source Graph alive.
#[derive(Clone, Debug)]
pub struct SelectedFocusSlot {
    frame: Weak<dyn SelectedFocusFrame>,
    index: usize,
}
impl SelectedFocusSlot {
    /// Resolve the still-active source issuer; index authority stays with it.
    pub fn issued(&self) -> Result<(Arc<dyn SelectedFocusFrame>, usize)> {
        Ok((
            self.frame
                .upgrade()
                .ok_or_else(|| Error::Invalid("Selected focus frame has ended".into()))?,
            self.index,
        ))
    }
    pub(crate) fn begin_nested(&self) -> Result<Arc<dyn SelectedValueFrame>> {
        let (frame, _) = self.issued()?;
        frame.begin_nested()
    }
    pub(crate) fn datatype(&self, expected: &NamedNode) -> Result<bool> {
        let (frame, index) = self.issued()?;
        frame.datatype(index, expected)
    }
    pub(crate) fn retain_literal(&self) -> Result<Option<SelectedLiteral>> {
        let (frame, index) = self.issued()?;
        frame.retain_literal(index)
    }
    pub(crate) fn present(&self, role: ReportTermRole) -> Result<String> {
        let (frame, index) = self.issued()?;
        frame.present(index, role)
    }
}

/// Stack-only inherited source subject. Provenance is not RDF equality.
#[derive(Clone, Copy)]
pub enum SelectedSubjectRef<'a> {
    /// A selected parent value.
    Value(&'a SelectedSlot),
    /// A selected target focus, including ordinary URI/BNode source owners.
    Focus(&'a SelectedFocusSlot),
}
