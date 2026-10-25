//! Immutable lexical owners for projection, independent of dictionary guards.
use super::small_string::SmallString;
use std::sync::Arc;

/// An immutable lexical value that owns inline, shared dictionary, or formatted text.
///
/// Cloning dictionary text shares its allocation. Its lifetime does not depend on
/// the storage reader or a held dictionary lock. No mutable dictionary view escapes.
#[derive(Clone, Debug)]
pub struct TermText(TextOwner);

#[derive(Clone, Debug)]
enum TextOwner {
    Inline(SmallString),
    Shared(Arc<str>),
    Owned(String),
}

impl TermText {
    /// Borrows the lexical text without copying its payload.
    pub fn as_str(&self) -> &str {
        match &self.0 {
            TextOwner::Inline(value) => value.as_str(),
            TextOwner::Shared(value) => value,
            TextOwner::Owned(value) => value,
        }
    }

    pub(crate) fn inline(value: SmallString) -> Self {
        Self(TextOwner::Inline(value))
    }

    pub(crate) fn shared(value: Arc<str>) -> Self {
        Self(TextOwner::Shared(value))
    }

    pub(crate) fn owned(value: String) -> Self {
        Self(TextOwner::Owned(value))
    }
}
