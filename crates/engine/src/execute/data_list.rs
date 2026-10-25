//! Checked borrowed list reads; retain cursors and counters, never list members.

use num_bigint::BigInt;
use oxigraph::model::*;

use crate::graph::{RDF, iri};
use crate::{Error, GraphRead, Result};

pub(super) struct DataList<'a, G: GraphRead + ?Sized> {
    data: &'a G,
    head: &'a Term,
    pub(super) length: BigInt,
}

impl<'a, G: GraphRead + ?Sized> DataList<'a, G> {
    pub(super) fn try_new(data: &'a G, head: &'a Term) -> Result<Option<Self>> {
        let mut list = Self {
            data,
            head,
            length: BigInt::from(0),
        };
        let nil = Term::from(iri(RDF, "nil"));
        let mut cursor = head.clone();
        let mut checkpoint = cursor.clone();
        // Brent cycle detection retains two nodes. Counters grow only logarithmically.
        let mut power = BigInt::from(1);
        let mut distance = BigInt::from(0);
        while cursor != nil {
            let Some((_, next)) = list.cell(&cursor)? else {
                return Ok(None);
            };
            list.length += 1u8;
            cursor = next;
            distance += 1u8;
            if cursor == checkpoint {
                return Ok(None);
            }
            if distance == power {
                checkpoint = cursor.clone();
                power *= 2u8;
                distance = BigInt::from(0);
            }
        }
        for name in ["first", "rest"] {
            if data
                .matching_quads(
                    Some(iri(RDF, "nil").as_ref().into()),
                    Some(iri(RDF, name).as_ref().into()),
                    None,
                    None,
                )
                .next()
                .transpose()?
                .is_some()
            {
                return Ok(None);
            }
        }
        Ok(Some(list))
    }

    fn object(
        &self,
        node: NamedOrBlankNodeRef<'_>,
        predicate: NamedNodeRef<'_>,
    ) -> Result<Option<Term>> {
        let mut value = None;
        let mut ambiguous = false;
        for quad in self
            .data
            .matching_quads(Some(node.into()), Some(predicate.into()), None, None)
        {
            let object = quad?.object;
            if let Some(previous) = &value {
                ambiguous |= previous != &object;
            } else {
                value = Some(object);
            }
        }
        Ok(if ambiguous { None } else { value })
    }

    fn cell(&self, cursor: &Term) -> Result<Option<(Term, Term)>> {
        let node = match cursor {
            Term::NamedNode(node) => NamedOrBlankNodeRef::from(node.as_ref()),
            Term::BlankNode(node) => NamedOrBlankNodeRef::from(node.as_ref()),
            Term::Literal(_) => return Ok(None),
        };
        let first = self.object(node, iri(RDF, "first").as_ref())?;
        let rest = self.object(node, iri(RDF, "rest").as_ref())?;
        Ok(first.zip(rest))
    }

    fn checked_cell(&self, cursor: &Term) -> Result<(Term, Term)> {
        self.cell(cursor)?
            .ok_or_else(|| Error::Runtime("List changed during validation".into()))
    }

    pub(super) fn members(&self) -> impl Iterator<Item = Result<Term>> + '_ {
        let mut cursor = self.head.clone();
        let nil = Term::from(iri(RDF, "nil"));
        std::iter::from_fn(move || {
            if cursor == nil {
                return None;
            }
            Some(match self.checked_cell(&cursor) {
                Ok((member, next)) => {
                    cursor = next;
                    Ok(member)
                }
                Err(error) => {
                    cursor = nil.clone();
                    Err(error)
                }
            })
        })
    }

    pub(super) fn has_duplicates(&self) -> Result<bool> {
        let nil = Term::from(iri(RDF, "nil"));
        let mut cursor = self.head.clone();
        while cursor != nil {
            let (member, next) = self.checked_cell(&cursor)?;
            let mut earlier = self.head.clone();
            // Pairwise exact-term probes use constant node state instead of a list-sized set.
            while earlier != cursor {
                let (previous, rest) = self.checked_cell(&earlier)?;
                if previous == member {
                    return Ok(true);
                }
                earlier = rest;
            }
            cursor = next;
        }
        Ok(false)
    }
}
