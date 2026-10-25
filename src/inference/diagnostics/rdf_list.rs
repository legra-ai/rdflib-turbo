//! Stable first-edge witnesses for append-only RDFLib-style list reads.

use std::cell::RefCell;
use std::collections::HashMap;

use oxigraph::model::Term;

use super::objects;
use crate::graph::{RDF, iri};
use crate::inference::backend::Graph;
use crate::inference::diagnostic_datatypes::list_truthiness;
use crate::{Error, Result};

#[derive(Clone, Default)]
struct SelectedEdges {
    first: Option<Term>,
    rest: Option<Term>,
}

pub(in crate::inference) struct ListWitnesses {
    // Diagnostic schema traversal state: at most two selected edges per touched
    // list cell, not an index or copy of ordinary graph/equality statements.
    edges: RefCell<HashMap<Term, SelectedEdges>>,
    integer_limit: usize,
}

impl ListWitnesses {
    pub(in crate::inference) fn new(integer_limit: usize) -> Self {
        Self {
            edges: RefCell::new(HashMap::new()),
            integer_limit,
        }
    }
    pub(super) fn integer_limit(&self) -> usize {
        self.integer_limit
    }

    fn read(&self, data: &impl Graph, node: &Term) -> Result<SelectedEdges> {
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

pub(in crate::inference) struct RdfList<'a, G> {
    data: &'a G,
    witnesses: &'a ListWitnesses,
    head: Term,
    pub len: usize,
}

impl<'a, G: Graph> RdfList<'a, G> {
    pub fn new(data: &'a G, witnesses: &'a ListWitnesses, head: Term) -> Result<Self> {
        let mut cursor = Cursor {
            data,
            witnesses,
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
            head,
            len,
        })
    }

    pub fn cursor(&self) -> Cursor<'a, G> {
        // The immutable round was validated above; replay the same witnesses.
        Cursor {
            data: self.data,
            witnesses: self.witnesses,
            current: Some(self.head.clone()),
            guard: None,
        }
    }
}

pub(in crate::inference) struct Cursor<'a, G> {
    data: &'a G,
    witnesses: &'a ListWitnesses,
    current: Option<Term>,
    guard: Option<CycleGuard>,
}

impl<'a, G: Graph> Cursor<'a, G> {
    pub fn tail(&self) -> Self {
        Self {
            data: self.data,
            witnesses: self.witnesses,
            current: self.current.clone(),
            guard: None,
        }
    }

    pub fn next(&mut self) -> Result<Option<Term>> {
        while let Some(node) = self.current.take() {
            // RDFLib uses cached default Python values, not SPARQL EBV or a
            // special rdf:nil branch. A falsy head performs no graph read.
            if !list_truthiness::truthy(&node, self.witnesses.integer_limit) {
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
