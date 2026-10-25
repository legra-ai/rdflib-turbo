//! A query-local union key leases the existing immutable source quad.
use super::{EncodedQuad, QuadIterator, QuadListNode};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// An opaque lease compared by full canonical SPO, within one source owner.
pub(crate) struct MemoryUnionKey(Arc<QuadListNode>);

impl MemoryUnionKey {
    pub(super) fn same_spo(left: &QuadListNode, right: &QuadListNode) -> bool {
        left.quad.subject == right.quad.subject
            && left.quad.predicate == right.quad.predicate
            && left.quad.object == right.quad.object
    }

    pub(super) fn hash_node<H: Hasher>(node: &QuadListNode, state: &mut H) {
        node.quad.subject.hash(state);
        node.quad.predicate.hash(state);
        node.quad.object.hash(state);
    }

    #[cfg(all(test, feature = "exact-term-storage"))]
    pub(super) fn from_node(node: Arc<QuadListNode>) -> Self {
        Self(node)
    }

    #[cfg(all(test, feature = "exact-term-storage"))]
    pub(super) fn owner(&self) -> &Arc<QuadListNode> {
        &self.0
    }
}

impl PartialEq for MemoryUnionKey {
    fn eq(&self, other: &Self) -> bool {
        Self::same_spo(&self.0, &other.0)
    }
}

impl Eq for MemoryUnionKey {}

impl Hash for MemoryUnionKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Self::hash_node(&self.0, state);
    }
}

/// Physical matching rows and their existing source-node ownership.
pub(crate) struct MemoryUnionCandidates<'a> {
    source: QuadIterator<'a>,
}

impl<'a> MemoryUnionCandidates<'a> {
    pub(super) fn new(source: QuadIterator<'a>) -> Self {
        Self { source }
    }
}

impl Iterator for MemoryUnionCandidates<'_> {
    type Item = (EncodedQuad, MemoryUnionKey);

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.source.next_node()?;
        let quad = node.quad.clone();
        Some((quad, MemoryUnionKey(node)))
    }
}
