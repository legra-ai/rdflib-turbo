//! Private immutable records shared by the six independently ordered indexes.
use crate::interning::*;
use std::borrow::Borrow;
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::sync::Arc;

type Spog = (
    InternedNamedOrBlankNode,
    InternedNamedNode,
    InternedTerm,
    InternedGraphName,
);
type Parts<'a> = (
    InternedNamedOrBlankNode,
    &'a InternedNamedNode,
    &'a InternedTerm,
    &'a InternedGraphName,
);
/// Borrowed canonical term identity without allocating or projecting tuple views.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
pub(super) enum RoleRef<'a> {
    Named(&'a InternedNamedNode),
    Blank(&'a InternedBlankNode),
    Literal(&'a InternedLiteral),
    #[cfg(feature = "rdf-12")]
    Triple(&'a InternedTerm),
}
impl<'a> From<&'a InternedTerm> for RoleRef<'a> {
    fn from(term: &'a InternedTerm) -> Self {
        match term {
            InternedTerm::NamedNode(t) => Self::Named(t),
            InternedTerm::BlankNode(t) => Self::Blank(t),
            InternedTerm::Literal(t) => Self::Literal(t),
            #[cfg(feature = "rdf-12")]
            InternedTerm::Triple(_) => Self::Triple(term),
        }
    }
}
impl Hash for RoleRef<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::Named(t) => InternedTerm::NamedNode(**t).hash(state),
            Self::Blank(t) => InternedTerm::BlankNode(**t).hash(state),
            Self::Literal(t) => InternedTerm::Literal(**t).hash(state),
            #[cfg(feature = "rdf-12")]
            Self::Triple(t) => t.hash(state),
        }
    }
}
impl RoleRef<'_> {
    pub(super) fn decode_from<'a>(self, interner: &'a Interner) -> crate::TermRef<'a> {
        match self {
            Self::Named(t) => t.decode_from(interner).into(),
            Self::Blank(t) => t.decode_from(interner).into(),
            Self::Literal(t) => t.decode_from(interner).into(),
            #[cfg(feature = "rdf-12")]
            Self::Triple(t) => t.decode_from(interner),
        }
    }
}
#[derive(Debug, Clone)]
pub(super) struct Common {
    subject: InternedTerm,
    object: InternedTerm,
    graph: InternedGraphName,
    predicate: InternedNamedNode,
}
#[derive(Debug, Clone)]
pub(super) struct Full {
    fields: (InternedTerm, InternedTerm, InternedTerm, InternedGraphName),
}
#[derive(Debug, Clone)]
pub(super) enum QuadRecord {
    Common(Common),
    GeneralizedPredicate(Box<Full>),
}
impl QuadRecord {
    pub(super) fn from_spog((s, p, o, g): Spog) -> Self {
        let s = match s {
            InternedNamedOrBlankNode::NamedNode(t) => InternedTerm::NamedNode(t),
            InternedNamedOrBlankNode::BlankNode(t) => InternedTerm::BlankNode(t),
        };
        Self::from_generalized((s, InternedTerm::NamedNode(p), o, g))
    }
    pub(super) fn from_generalized(
        (s, p, o, g): (InternedTerm, InternedTerm, InternedTerm, InternedGraphName),
    ) -> Self {
        match p {
            InternedTerm::NamedNode(predicate) => Self::Common(Common {
                subject: s,
                object: o,
                graph: g,
                predicate,
            }),
            p => Self::GeneralizedPredicate(Box::new(Full {
                fields: (s, p, o, g),
            })),
        }
    }
    pub(super) fn from_gspo(
        (g, s, p, o): (
            InternedGraphName,
            InternedNamedOrBlankNode,
            InternedNamedNode,
            InternedTerm,
        ),
    ) -> Self {
        Self::from_spog((s, p, o, g))
    }
    pub(super) fn generalized_parts(
        &self,
    ) -> (RoleRef<'_>, RoleRef<'_>, RoleRef<'_>, &InternedGraphName) {
        match self {
            Self::Common(c) => (
                (&c.subject).into(),
                RoleRef::Named(&c.predicate),
                (&c.object).into(),
                &c.graph,
            ),
            Self::GeneralizedPredicate(f) => (
                (&f.fields.0).into(),
                (&f.fields.1).into(),
                (&f.fields.2).into(),
                &f.fields.3,
            ),
        }
    }
    pub(super) fn parts(&self) -> Parts<'_> {
        let Self::Common(c) = self else {
            panic!("Generalized predicate reached strict RDF dataset view")
        };
        let subject = match c.subject {
            InternedTerm::NamedNode(t) => InternedNamedOrBlankNode::NamedNode(t),
            InternedTerm::BlankNode(t) => InternedNamedOrBlankNode::BlankNode(t),
            _ => panic!("Generalized subject reached strict RDF dataset view"),
        };
        (subject, &c.predicate, &c.object, &c.graph)
    }
    pub(super) fn as_tuple(&self) -> Spog {
        let (s, p, o, g) = self.parts();
        (s, *p, o.clone(), *g)
    }
}
impl PartialEq for QuadRecord {
    fn eq(&self, other: &Self) -> bool {
        self.generalized_parts() == other.generalized_parts()
    }
}
impl Eq for QuadRecord {}
impl Hash for QuadRecord {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.generalized_parts().hash(state);
    }
}
impl Ord for QuadRecord {
    fn cmp(&self, other: &Self) -> Ordering {
        let a = self.generalized_parts();
        let b = other.generalized_parts();
        (a.3, a.0, a.1, a.2).cmp(&(b.3, b.0, b.1, b.2))
    }
}
impl PartialOrd for QuadRecord {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

macro_rules! key {
    ($name:ident, $tuple:ty, ($a:ident,$b:ident,$c:ident,$d:ident), ($s:ident,$p:ident,$o:ident,$g:ident), ($i:tt,$j:tt,$k:tt,$l:tt)) => {
        #[derive(Debug, Clone)]
        pub(super) struct $name {
            pub(super) record: Arc<QuadRecord>,
        }
        impl $name {
            pub(super) fn new(record: Arc<QuadRecord>) -> Self {
                Self { record }
            }
        }
        impl Deref for $name {
            type Target = QuadRecord;
            fn deref(&self) -> &QuadRecord {
                &self.record
            }
        }
        impl PartialEq for $name {
            fn eq(&self, other: &Self) -> bool {
                self.record == other.record
            }
        }
        impl Eq for $name {}
        impl Ord for $name {
            fn cmp(&self, other: &Self) -> Ordering {
                let a = self.record.generalized_parts();
                let b = other.record.generalized_parts();
                (&a.$i, &a.$j, &a.$k, &a.$l).cmp(&(&b.$i, &b.$j, &b.$k, &b.$l))
            }
        }
        impl PartialOrd for $name {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }
    };
}
key!(
    GspoKey,
    (
        InternedGraphName,
        InternedNamedOrBlankNode,
        InternedNamedNode,
        InternedTerm
    ),
    (g, s, p, o),
    (s, p, o, g),
    (3, 0, 1, 2)
);
key!(
    GposKey,
    (
        InternedGraphName,
        InternedNamedNode,
        InternedTerm,
        InternedNamedOrBlankNode
    ),
    (g, p, o, s),
    (s, p, o, g),
    (3, 1, 2, 0)
);
key!(
    GospKey,
    (
        InternedGraphName,
        InternedTerm,
        InternedNamedOrBlankNode,
        InternedNamedNode
    ),
    (g, o, s, p),
    (s, p, o, g),
    (3, 2, 0, 1)
);
key!(SpogKey, Spog, (s, p, o, g), (s, p, o, g), (0, 1, 2, 3));
key!(
    PosgKey,
    (
        InternedNamedNode,
        InternedTerm,
        InternedNamedOrBlankNode,
        InternedGraphName
    ),
    (p, o, s, g),
    (s, p, o, g),
    (1, 2, 0, 3)
);
key!(
    OspgKey,
    (
        InternedTerm,
        InternedNamedOrBlankNode,
        InternedNamedNode,
        InternedGraphName
    ),
    (o, s, p, g),
    (s, p, o, g),
    (2, 0, 1, 3)
);
impl Borrow<QuadRecord> for GspoKey {
    fn borrow(&self) -> &QuadRecord {
        &self.record
    }
}
impl GposKey {
    pub(super) fn from_tuple(
        (g, p, o, s): (
            InternedGraphName,
            InternedNamedNode,
            InternedTerm,
            InternedNamedOrBlankNode,
        ),
    ) -> Self {
        Self::new(Arc::new(QuadRecord::from_spog((s, p, o, g))))
    }
}
impl GospKey {
    pub(super) fn from_tuple(
        (g, o, s, p): (
            InternedGraphName,
            InternedTerm,
            InternedNamedOrBlankNode,
            InternedNamedNode,
        ),
    ) -> Self {
        Self::new(Arc::new(QuadRecord::from_spog((s, p, o, g))))
    }
}
impl SpogKey {
    pub(super) fn from_tuple((s, p, o, g): Spog) -> Self {
        Self::new(Arc::new(QuadRecord::from_spog((s, p, o, g))))
    }
}
impl PosgKey {
    pub(super) fn from_tuple(
        (p, o, s, g): (
            InternedNamedNode,
            InternedTerm,
            InternedNamedOrBlankNode,
            InternedGraphName,
        ),
    ) -> Self {
        Self::new(Arc::new(QuadRecord::from_spog((s, p, o, g))))
    }
}
impl OspgKey {
    pub(super) fn from_tuple(
        (o, s, p, g): (
            InternedTerm,
            InternedNamedOrBlankNode,
            InternedNamedNode,
            InternedGraphName,
        ),
    ) -> Self {
        Self::new(Arc::new(QuadRecord::from_spog((s, p, o, g))))
    }
}
