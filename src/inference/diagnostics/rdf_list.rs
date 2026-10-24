//! Stable first-edge witnesses for append-only RDFLib-style list reads.

use std::cell::RefCell;
use std::collections::HashMap;

use oxigraph::model::Term;

use super::objects;
use crate::graph::{RDF, iri};
use crate::inference::backend::Graph;
use crate::inference::diagnostic_datatypes::{self, LexicalContext};
use crate::inference::generalized::Tuples;
use crate::{Error, Result};

#[derive(Clone, Default)]
struct SelectedEdges {
    first: Option<Term>,
    rest: Option<Term>,
}

#[derive(Default)]
pub(super) struct ListWitnesses {
    // Diagnostic schema traversal state: at most two selected edges per touched
    // list cell, not an index or copy of ordinary graph/equality statements.
    edges: RefCell<HashMap<Term, SelectedEdges>>,
}

impl ListWitnesses {
    fn read(&self, data: &Tuples<'_, impl Graph>, node: &Term) -> Result<SelectedEdges> {
        let mut selected = self.edges.borrow().get(node).cloned().unwrap_or_default();
        if selected.first.is_some() && selected.rest.is_some() {
            return Ok(selected);
        }
        // Absence is not a witness: a later round can infer a missing edge.
        if selected.first.is_none() {
            selected.first = objects(data, node, &iri(RDF, "first")).next().transpose()?;
        }
        if selected.rest.is_none() {
            selected.rest = objects(data, node, &iri(RDF, "rest")).next().transpose()?;
        }
        if selected.first.is_some() || selected.rest.is_some() {
            self.edges
                .borrow_mut()
                .insert(node.clone(), selected.clone());
        }
        Ok(selected)
    }
}

pub(super) struct RdfList<'a, 'graph, G> {
    data: &'a Tuples<'graph, G>,
    witnesses: &'a ListWitnesses,
    context: &'a LexicalContext,
    head: Term,
    pub len: usize,
}

impl<'a, 'graph, G: Graph> RdfList<'a, 'graph, G> {
    pub fn new(
        data: &'a Tuples<'graph, G>,
        witnesses: &'a ListWitnesses,
        head: Term,
        context: &'a LexicalContext,
    ) -> Result<Self> {
        let mut cursor = Cursor {
            data,
            witnesses,
            context,
            current: Some(head.clone()),
            guard: Some(CycleGuard {
                anchor: head.clone(),
                power: 1,
                distance: 0,
            }),
        };
        let mut len = 0_usize;
        while cursor.next()?.is_some() {
            len = len
                .checked_add(1)
                .ok_or_else(|| Error::Invalid("RDF list length overflow".into()))?;
        }
        Ok(Self {
            data,
            witnesses,
            context,
            head,
            len,
        })
    }

    pub fn cursor(&self) -> Cursor<'a, 'graph, G> {
        // The immutable round was validated above; replay the same witnesses.
        Cursor {
            data: self.data,
            witnesses: self.witnesses,
            context: self.context,
            current: Some(self.head.clone()),
            guard: None,
        }
    }
}

pub(super) struct Cursor<'a, 'graph, G> {
    data: &'a Tuples<'graph, G>,
    witnesses: &'a ListWitnesses,
    context: &'a LexicalContext,
    current: Option<Term>,
    guard: Option<CycleGuard>,
}

impl<'a, 'graph, G: Graph> Cursor<'a, 'graph, G> {
    pub fn tail(&self) -> Self {
        Self {
            data: self.data,
            witnesses: self.witnesses,
            context: self.context,
            current: self.current.clone(),
            guard: None,
        }
    }

    pub fn next(&mut self) -> Result<Option<Term>> {
        while let Some(node) = self.current.take() {
            // RDFLib uses Python node truthiness, not a special rdf:nil branch.
            let truthy = match &node {
                Term::Literal(literal) => diagnostic_datatypes::truthy(literal, self.context),
                _ => !super::text(&node).is_empty(),
            };
            if !truthy {
                return Ok(None);
            }
            let selected = self.witnesses.read(self.data, &node)?;
            self.current = selected.rest;
            if let (Some(guard), Some(next)) = (&mut self.guard, &self.current) {
                guard.advance(next)?;
            }
            if selected.first.is_some() {
                return Ok(selected.first);
            }
        }
        Ok(None)
    }
}

struct CycleGuard {
    anchor: Term,
    power: usize,
    distance: usize,
}

impl CycleGuard {
    fn advance(&mut self, node: &Term) -> Result<()> {
        if &self.anchor == node {
            return Err(Error::Invalid(
                "List contains a recursive rdf:rest reference".into(),
            ));
        }
        self.distance += 1;
        if self.distance == self.power {
            self.anchor = node.clone();
            self.power = self.power.saturating_mul(2);
            self.distance = 0;
        }
        Ok(())
    }
}
