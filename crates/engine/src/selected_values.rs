//! Invocation-scoped selected value leases; reported owners never retain a frame.
use crate::{BuiltinBooleanState, Error, Result};
use oxigraph::model::{NamedNode, Term};
use std::{
    any::Any,
    fmt::Debug,
    sync::{Arc, Weak},
};

/// Call-local view of a compiled sequence containing only named predicates.
/// The existing private path owns all steps; this view never copies an AST.
#[derive(Clone, Copy)]
pub struct SelectedPathRef<'a> {
    steps: &'a [crate::path::Path],
}
impl<'a> SelectedPathRef<'a> {
    pub(crate) fn predicate_sequence(path: &'a crate::path::Path) -> Option<Self> {
        let crate::path::Path::Sequence(steps) = path else {
            return None;
        };
        (steps.len() >= 2
            && steps
                .iter()
                .all(|p| matches!(p, crate::path::Path::Predicate(_))))
        .then_some(Self { steps })
    }
    pub(crate) fn count_path(path: &'a crate::path::Path) -> Option<Self> {
        if matches!(path, crate::path::Path::Predicate(_)) {
            Some(Self {
                steps: std::slice::from_ref(path),
            })
        } else {
            Self::predicate_sequence(path)
        }
    }
    /// Borrow the validated predicates in their original sequence order.
    pub fn predicates(self) -> impl ExactSizeIterator<Item = &'a NamedNode> + Clone {
        self.steps.iter().map(|step| {
            let crate::path::Path::Predicate(predicate) = step else {
                unreachable!("Selected sequence contains a non-predicate step");
            };
            predicate
        })
    }
}

/// Explicit ordinary-source admission, absent on lexical/native readers.
pub trait SelectedValueSource: Debug + Send + Sync {
    /// Start one active shape invocation's value-node acquisition.
    fn begin_frame(&self) -> Result<Arc<dyn SelectedValueFrame>>;
}

/// Native slots and their issuer are valid only during this active invocation.
pub trait SelectedValueFrame: Debug + Send + Sync {
    /// Checked source-specific issuer, never an RDF identity.
    fn as_any(&self) -> &dyn Any;
    /// Issue a fresh nested shape frame under this actual source capability.
    fn begin_nested(&self) -> Result<Arc<dyn SelectedValueFrame>>;
    /// Acquire one requested predicate projection, without assessment getters.
    fn predicate(
        &self,
        focus: &Term,
        predicate: &NamedNode,
        subject: Option<crate::SelectedSubjectRef<'_>>,
    ) -> Result<Vec<SelectedTerm>>;
    /// Acquire an entire named-predicate sequence with actual source set semantics.
    fn sequence(
        &self,
        focus: &Term,
        path: SelectedPathRef<'_>,
        subject: Option<crate::SelectedSubjectRef<'_>>,
    ) -> Result<Vec<SelectedTerm>>;
    /// Acquire and retain the actual source set without inspecting its members.
    fn cardinality(
        &self,
        focus: &Term,
        path: SelectedPathRef<'_>,
        subject: Option<crate::SelectedSubjectRef<'_>>,
    ) -> Result<usize>;
    /// Read the actual selected object's datatype properties at its consumer.
    fn datatype(&self, slot: usize, expected: &NamedNode) -> Result<bool>;
    /// Retain only this actual reported object, independently of this frame.
    fn retain(&self, slot: usize) -> Result<SelectedLiteral>;
    /// Execute one actual report-format consumer at violation creation.
    fn present(&self, slot: usize, role: ReportTermRole) -> Result<String>;
    /// End issuance and release the active objects, also on error unwinding.
    fn close(&self);
}

/// Direct report occurrence; focus and value can present the same owner twice.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReportTermRole {
    /// The actual result focus.
    Focus,
    /// The actual result value.
    Value,
}
impl ReportTermRole {
    /// Exact SHACL predicate admitted by this occurrence role.
    pub fn predicate(self) -> &'static str {
        match self {
            Self::Focus => "http://www.w3.org/ns/shacl#focusNode",
            Self::Value => "http://www.w3.org/ns/shacl#value",
        }
    }
}

/// One requested RDF value with an optional frame-issued Boolean owner index.
pub struct SelectedTerm {
    /// Actual selected RDF word, not a requirement to match raw iteration.
    pub term: Term,
    /// Index issued by the same frame that returned this value.
    pub slot: Option<usize>,
}

/// A source-specific checked reported owner, without a graph or batch lifetime.
pub trait SelectedLiteralOwner: Any + Debug + Send + Sync {
    /// Bindings must check the exact native owner type before resolving it.
    fn as_any(&self) -> &dyn Any;
}

/// Sparse retained report object; never an owner of all acquired values.
#[derive(Clone, Debug)]
pub struct SelectedLiteral(pub Arc<dyn SelectedLiteralOwner>);

/// A lease borrows an active issuer, so retaining it cannot extend frame lifetime.
#[derive(Clone, Debug)]
pub struct SelectedSlot {
    frame: Weak<dyn SelectedValueFrame>,
    slot: usize,
}
impl SelectedSlot {
    pub(crate) fn new(frame: &Arc<dyn SelectedValueFrame>, slot: usize) -> Self {
        Self {
            frame: Arc::downgrade(frame),
            slot,
        }
    }
    /// Borrow the still-live issuer and slot; the source must validate its own owner.
    pub fn issued(&self) -> Result<(Arc<dyn SelectedValueFrame>, usize)> {
        Ok((self.frame()?, self.slot))
    }
    fn frame(&self) -> Result<Arc<dyn SelectedValueFrame>> {
        self.frame
            .upgrade()
            .ok_or_else(|| Error::Invalid("Selected value frame has ended".into()))
    }
    fn begin_nested(&self) -> Result<Arc<dyn SelectedValueFrame>> {
        self.frame()?.begin_nested()
    }
    pub(crate) fn datatype(&self, expected: &NamedNode) -> Result<bool> {
        self.frame()?.datatype(self.slot, expected)
    }
    fn retain(&self) -> Result<SelectedLiteral> {
        self.frame()?.retain(self.slot)
    }
    fn present(&self, role: ReportTermRole) -> Result<String> {
        self.frame()?.present(self.slot, role)
    }
}

/// Serialized assessment and live selected ownership are separate authorities.
#[derive(Clone, Debug)]
pub enum BooleanObservation {
    /// Checked serialized state from an explicitly assessed reader.
    Attested(BuiltinBooleanState),
    /// A live source lease, read only by its actual consumer.
    Selected(SelectedSlot),
    /// A source target focus, independent of Boolean value eligibility.
    Focus(crate::SelectedFocusSlot),
    /// One object retained solely for an actual direct SH.value occurrence.
    Reported(SelectedLiteral),
}
impl BooleanObservation {
    pub(crate) fn begin_nested(&self) -> Result<Option<Arc<dyn SelectedValueFrame>>> {
        match self {
            Self::Selected(slot) => slot.begin_nested().map(Some),
            Self::Focus(slot) => slot.begin_nested().map(Some),
            Self::Attested(_) => Ok(None),
            Self::Reported(_) => Err(Error::Invalid(
                "Reported owner cannot issue a value frame".into(),
            )),
        }
    }
    pub(crate) fn subject(&self) -> Option<crate::SelectedSubjectRef<'_>> {
        match self {
            Self::Selected(slot) => Some(crate::SelectedSubjectRef::Value(slot)),
            Self::Focus(slot) => Some(crate::SelectedSubjectRef::Focus(slot)),
            _ => None,
        }
    }
    pub(crate) fn present(&self, role: ReportTermRole) -> Result<Option<String>> {
        match self {
            Self::Selected(slot) => slot.present(role).map(Some),
            Self::Focus(slot) => slot.present(role).map(Some),
            _ => Ok(None),
        }
    }
    pub(crate) fn for_report(&self) -> Result<Option<Self>> {
        Ok(match self {
            Self::Selected(slot) => Some(Self::Reported(slot.retain()?)),
            Self::Focus(slot) => slot.retain_literal()?.map(Self::Reported),
            other => Some(other.clone()),
        })
    }
}

pub(crate) enum SelectedFrameGuard<'a> {
    Owned(Arc<dyn SelectedValueFrame>),
    Borrowed(&'a Arc<dyn SelectedValueFrame>),
}
impl SelectedFrameGuard<'_> {
    pub fn frame(&self) -> &Arc<dyn SelectedValueFrame> {
        match self {
            Self::Owned(frame) => frame,
            Self::Borrowed(frame) => frame,
        }
    }
}
impl Drop for SelectedFrameGuard<'_> {
    fn drop(&mut self) {
        if let Self::Owned(frame) = self {
            frame.close();
        }
    }
}
