//! Unordered missing-head frontier retires consumed storage as output grows.
use std::cmp::Ordering;
use std::collections::BTreeSet;

use oxigraph::model::{GeneralizedQuad, GraphNameRef, LiteralRef, TermRef};

use super::HeadDisposition;
use super::backend::Graph;
use crate::Result;

#[derive(Default, Debug, Eq, PartialEq)]
pub(super) struct Pending(BTreeSet<QuadKey>);

impl Pending {
    /// Direct publications preserve their separate reference policy.
    pub(super) fn insert(&mut self, quad: GeneralizedQuad) -> bool {
        self.0.insert(QuadKey(quad))
    }

    pub(super) fn push(
        &mut self,
        data: &impl Graph,
        quad: GeneralizedQuad,
    ) -> Result<HeadDisposition> {
        let quad = QuadKey(quad);
        if self.0.contains(&quad) || data.contains(quad.0.as_ref())? {
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
        if !self.0.contains(&quad) && !data.contains_destination(quad.0.as_ref())? {
            self.0.insert(quad);
        }
        Ok(())
    }

    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[cfg(test)]
    pub(super) fn compare_quads(left: &GeneralizedQuad, right: &GeneralizedQuad) -> Ordering {
        QuadKey::compare(left, right)
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.0.len()
    }

    #[cfg(test)]
    pub(super) fn contains(&self, quad: &GeneralizedQuad) -> bool {
        self.0.contains(&QuadKey(quad.clone()))
    }
}

impl IntoIterator for Pending {
    type Item = GeneralizedQuad;
    type IntoIter = std::iter::Map<
        std::collections::btree_set::IntoIter<QuadKey>,
        fn(QuadKey) -> GeneralizedQuad,
    >;

    fn into_iter(self) -> Self::IntoIter {
        // This frontier promises no iteration order; tree ordering is private.
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
        self.0.iter().map(|quad| &quad.0)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct QuadKey(GeneralizedQuad);

impl QuadKey {
    fn compare(left: &GeneralizedQuad, right: &GeneralizedQuad) -> Ordering {
        let left = left.as_ref();
        let right = right.as_ref();
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
