//! Unordered missing-head frontier retires consumed storage as output grows.
use std::borrow::Borrow;
use std::cmp::Ordering;
use std::collections::BTreeSet;

use oxigraph::model::{GeneralizedQuad, GeneralizedQuadRef, GraphNameRef, LiteralRef, TermRef};

use super::HeadDisposition;
use super::backend::Graph;
use crate::Result;
use oxigraph::sparql::LeasedInferenceTriple;

#[derive(Default, Debug)]
pub(super) struct Pending(BTreeSet<QuadKey>, BTreeSet<NativeKey>);

impl Pending {
    pub(super) fn push_head(&mut self, data: &impl Graph, head: Head) -> Result<HeadDisposition> {
        match head {
            Head::Owned(quad) => self.push(data, quad),
            Head::Native(head) => self.push_native(data, head),
        }
    }
    fn contains_ref(&self, quad: GeneralizedQuadRef<'_>) -> bool {
        let probe = Probe(quad);
        let probe: &dyn QuadView = &probe;
        self.0.contains(probe) || self.1.contains(probe)
    }

    pub(super) fn push_native(
        &mut self,
        data: &impl Graph,
        head: LeasedInferenceTriple,
    ) -> Result<HeadDisposition> {
        let head = NativeKey(head);
        let quad = head.quad();
        if self.contains_ref(quad) || data.contains(quad)? {
            return Ok(HeadDisposition::AlreadyPresent);
        }
        if quad.predicate.is_literal() {
            return Ok(HeadDisposition::RefusedLiteralPredicate);
        }
        self.1.insert(head);
        Ok(HeadDisposition::Accepted)
    }

    pub(super) fn into_heads(self) -> Heads {
        Heads {
            owned: self.0.into_iter().peekable(),
            native: self.1.into_iter().peekable(),
        }
    }

    #[cfg(all(test, feature = "allocation-profiling"))]
    pub(super) fn holder_sizes() -> (usize, usize) {
        (
            std::mem::size_of::<NativeKey>(),
            std::mem::size_of::<Head>(),
        )
    }

    #[cfg(test)]
    pub(super) fn heads(&self) -> impl Iterator<Item = GeneralizedQuadRef<'_>> {
        self.0
            .iter()
            .map(QuadView::quad)
            .chain(self.1.iter().map(QuadView::quad))
    }

    /// Direct publications preserve their separate reference policy.
    pub(super) fn insert(&mut self, quad: GeneralizedQuad) -> bool {
        if self.contains_ref(quad.as_ref()) {
            return false;
        }
        self.0.insert(QuadKey(quad))
    }

    pub(super) fn push(
        &mut self,
        data: &impl Graph,
        quad: GeneralizedQuad,
    ) -> Result<HeadDisposition> {
        let quad = QuadKey(quad);
        if self.contains_ref(quad.0.as_ref()) || data.contains(quad.0.as_ref())? {
            return Ok(HeadDisposition::AlreadyPresent);
        }
        if quad.0.predicate.is_literal() {
            return Ok(HeadDisposition::RefusedLiteralPredicate);
        }
        self.0.insert(quad);
        Ok(HeadDisposition::Accepted)
    }

    pub(super) fn publish(&mut self, data: &impl Graph, quad: GeneralizedQuad) -> Result<()> {
        let quad = QuadKey(quad);
        if !self.contains_ref(quad.0.as_ref()) && !data.contains_destination(quad.0.as_ref())? {
            self.0.insert(quad);
        }
        Ok(())
    }

    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty() && self.1.is_empty()
    }

    #[cfg(test)]
    pub(super) fn compare_quads(left: &GeneralizedQuad, right: &GeneralizedQuad) -> Ordering {
        QuadKey::compare(left, right)
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.0.len() + self.1.len()
    }

    #[cfg(test)]
    pub(super) fn contains(&self, quad: &GeneralizedQuad) -> bool {
        self.contains_ref(quad.as_ref())
    }
}

#[cfg(test)]
impl IntoIterator for Pending {
    type Item = GeneralizedQuad;
    type IntoIter = std::iter::Map<
        std::collections::btree_set::IntoIter<QuadKey>,
        fn(QuadKey) -> GeneralizedQuad,
    >;

    fn into_iter(self) -> Self::IntoIter {
        // This frontier promises no iteration order; tree ordering is private.
        assert!(
            self.1.is_empty(),
            "native frontier must be consumed through borrowed heads"
        );
        self.0.into_iter().map(|quad| quad.0)
    }
}

#[cfg(test)]
impl<'a> IntoIterator for &'a Pending {
    type Item = &'a GeneralizedQuad;
    type IntoIter = std::iter::Map<
        std::collections::btree_set::Iter<'a, QuadKey>,
        fn(&QuadKey) -> &GeneralizedQuad,
    >;

    fn into_iter(self) -> Self::IntoIter {
        assert!(
            self.1.is_empty(),
            "native frontier must be observed through borrowed heads"
        );
        self.0.iter().map(|quad| &quad.0)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct QuadKey(GeneralizedQuad);

impl QuadKey {
    fn compare(left: &GeneralizedQuad, right: &GeneralizedQuad) -> Ordering {
        Self::compare_refs(left.as_ref(), right.as_ref())
    }

    fn compare_refs(left: GeneralizedQuadRef<'_>, right: GeneralizedQuadRef<'_>) -> Ordering {
        Self::term(left.subject, right.subject)
            .then_with(|| Self::term(left.predicate, right.predicate))
            .then_with(|| Self::term(left.object, right.object))
            .then_with(|| Self::graph(left.graph_name, right.graph_name))
    }

    fn term(left: TermRef<'_>, right: TermRef<'_>) -> Ordering {
        match (left, right) {
            (TermRef::NamedNode(left), TermRef::NamedNode(right)) => {
                left.as_str().cmp(right.as_str())
            }
            (TermRef::BlankNode(left), TermRef::BlankNode(right)) => {
                left.as_str().cmp(right.as_str())
            }
            (TermRef::Literal(left), TermRef::Literal(right)) => Self::literal(left, right),
            _ => Self::rank(left).cmp(&Self::rank(right)),
        }
    }

    fn rank(term: TermRef<'_>) -> u8 {
        match term {
            TermRef::NamedNode(_) => 0,
            TermRef::BlankNode(_) => 1,
            TermRef::Literal(_) => 2,
        }
    }

    fn literal(left: LiteralRef<'_>, right: LiteralRef<'_>) -> Ordering {
        left.datatype()
            .as_str()
            .cmp(right.datatype().as_str())
            .then_with(|| left.language().cmp(&right.language()))
            .then_with(|| {
                left.has_explicit_datatype()
                    .cmp(&right.has_explicit_datatype())
            })
            .then_with(|| left.value().cmp(right.value()))
    }

    fn graph(left: GraphNameRef<'_>, right: GraphNameRef<'_>) -> Ordering {
        match (left, right) {
            (GraphNameRef::DefaultGraph, GraphNameRef::DefaultGraph) => Ordering::Equal,
            (GraphNameRef::DefaultGraph, _) => Ordering::Less,
            (_, GraphNameRef::DefaultGraph) => Ordering::Greater,
            (GraphNameRef::NamedNode(left), GraphNameRef::NamedNode(right)) => {
                left.as_str().cmp(right.as_str())
            }
            (GraphNameRef::BlankNode(left), GraphNameRef::BlankNode(right)) => {
                left.as_str().cmp(right.as_str())
            }
            (GraphNameRef::NamedNode(_), GraphNameRef::BlankNode(_)) => Ordering::Less,
            (GraphNameRef::BlankNode(_), GraphNameRef::NamedNode(_)) => Ordering::Greater,
        }
    }
}

impl Ord for QuadKey {
    fn cmp(&self, other: &Self) -> Ordering {
        Self::compare(&self.0, &other.0)
    }
}

impl PartialOrd for QuadKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

trait QuadView {
    fn quad(&self) -> GeneralizedQuadRef<'_>;
}
struct Probe<'a>(GeneralizedQuadRef<'a>);
impl QuadView for Probe<'_> {
    fn quad(&self) -> GeneralizedQuadRef<'_> {
        self.0
    }
}
impl QuadView for QuadKey {
    fn quad(&self) -> GeneralizedQuadRef<'_> {
        self.0.as_ref()
    }
}
impl PartialEq for dyn QuadView + '_ {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}
impl Eq for dyn QuadView + '_ {}
impl PartialOrd for dyn QuadView + '_ {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for dyn QuadView + '_ {
    fn cmp(&self, other: &Self) -> Ordering {
        QuadKey::compare_refs(self.quad(), other.quad())
    }
}
impl<'a> Borrow<dyn QuadView + 'a> for QuadKey {
    fn borrow(&self) -> &(dyn QuadView + 'a) {
        self
    }
}

#[derive(Debug)]
struct NativeKey(LeasedInferenceTriple);
impl QuadView for NativeKey {
    fn quad(&self) -> GeneralizedQuadRef<'_> {
        let triple = self.0.as_ref();
        GeneralizedQuadRef {
            subject: triple.subject,
            predicate: triple.predicate,
            object: triple.object,
            graph_name: GraphNameRef::DefaultGraph,
        }
    }
}
impl<'a> Borrow<dyn QuadView + 'a> for NativeKey {
    fn borrow(&self) -> &(dyn QuadView + 'a) {
        self
    }
}
impl PartialEq for NativeKey {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}
impl Eq for NativeKey {}
impl PartialOrd for NativeKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for NativeKey {
    fn cmp(&self, other: &Self) -> Ordering {
        QuadKey::compare_refs(self.quad(), other.quad())
    }
}

// Only one transient result lives on the stack. Native records in the retained
// tree remain the 24-byte lease holder; boxing owned rows would add a per-head allocation.
#[expect(
    clippy::large_enum_variant,
    reason = "separate compact trees retain heads, this enum only transfers one row"
)]
pub(super) enum Head {
    Owned(GeneralizedQuad),
    Native(LeasedInferenceTriple),
}
impl Head {
    pub(super) fn as_ref(&self) -> GeneralizedQuadRef<'_> {
        match self {
            Self::Owned(quad) => quad.as_ref(),
            Self::Native(head) => {
                let head = head.as_ref();
                GeneralizedQuadRef {
                    subject: head.subject,
                    predicate: head.predicate,
                    object: head.object,
                    graph_name: GraphNameRef::DefaultGraph,
                }
            }
        }
    }
}
pub(super) struct Heads {
    owned: std::iter::Peekable<std::collections::btree_set::IntoIter<QuadKey>>,
    native: std::iter::Peekable<std::collections::btree_set::IntoIter<NativeKey>>,
}
impl Iterator for Heads {
    type Item = Head;
    fn next(&mut self) -> Option<Self::Item> {
        let native = match (self.owned.peek(), self.native.peek()) {
            (Some(owned), Some(native)) => {
                QuadKey::compare_refs(owned.quad(), native.quad()).is_gt()
            }
            (None, Some(_)) => true,
            (_, None) => false,
        };
        if native {
            self.native.next().map(|head| Head::Native(head.0))
        } else {
            self.owned.next().map(|head| Head::Owned(head.0))
        }
    }
}

#[cfg(test)]
impl PartialEq for Pending {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.heads().all(|quad| other.contains_ref(quad))
    }
}
#[cfg(test)]
impl Eq for Pending {}
