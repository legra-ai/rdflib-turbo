//! Private immutable records shared by the six independently ordered indexes.
use crate::interning::*;
use std::borrow::Borrow;
use std::cmp::Ordering;
use std::ops::Deref;
use std::sync::Arc;

type Spog = (
    InternedNamedOrBlankNode,
    InternedNamedNode,
    InternedTerm,
    InternedGraphName,
);
type Parts<'a> = (
    &'a InternedNamedOrBlankNode,
    &'a InternedNamedNode,
    &'a InternedTerm,
    &'a InternedGraphName,
);
#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct QuadRecord {
    fields: Spog,
}
impl QuadRecord {
    pub(super) fn from_spog(fields: Spog) -> Self {
        Self { fields }
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
    pub(super) fn as_tuple(&self) -> &Spog {
        &self.fields
    }
    pub(super) fn parts(&self) -> Parts<'_> {
        (
            &self.fields.0,
            &self.fields.1,
            &self.fields.2,
            &self.fields.3,
        )
    }
}
impl Deref for QuadRecord {
    type Target = Spog;
    fn deref(&self) -> &Spog {
        &self.fields
    }
}
// This is the canonical GSPO order, also used by GspoKey's borrowed probes.
impl Ord for QuadRecord {
    fn cmp(&self, other: &Self) -> Ordering {
        let a = &self.fields;
        let b = &other.fields;
        (&a.3, &a.0, &a.1, &a.2).cmp(&(&b.3, &b.0, &b.1, &b.2))
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
                let a = self.record.as_tuple();
                let b = other.record.as_tuple();
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
