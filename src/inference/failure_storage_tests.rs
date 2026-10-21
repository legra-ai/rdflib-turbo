//! Fallible storage capability over a real insertion-journal snapshot.
use std::cell::Cell;

use oxigraph::model::*;
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults};
use oxrdf::dataset::InsertTransaction;

use crate::inference::backend::Graph;
use crate::{Error, Result};

pub(super) const FAILURE: &str = "synthetic storage read failure";

#[derive(Clone, PartialEq)]
pub(super) struct Scan {
    subject: Option<NamedOrBlankNode>,
    predicate: Option<NamedNode>,
    object: Option<Term>,
}

impl Scan {
    pub(super) fn new(
        subject: Option<&str>,
        predicate: Option<&str>,
        object: Option<&str>,
    ) -> Self {
        Self {
            subject: subject.map(|value| node(value).into()),
            predicate: predicate.map(node),
            object: object.map(|value| node(value).into()),
        }
    }
    fn selectors(
        s: Option<NamedOrBlankNodeRef<'_>>,
        p: Option<NamedNodeRef<'_>>,
        o: Option<TermRef<'_>>,
    ) -> Self {
        Self {
            subject: s.map(|value| value.into_owned()),
            predicate: p.map(|value| value.into_owned()),
            object: o.map(|value| value.into_owned()),
        }
    }
}

pub(super) enum Activation {
    Immediate,
    AfterEqualityEnumeration,
    AfterUnfilteredReadStarts,
}

pub(super) struct Fault {
    pub scan: Scan,
    pub activation: Activation,
    pub after_results: usize,
}

pub(super) struct Storage<'a, 'data> {
    data: &'a InsertTransaction<'data>,
    fault: Fault,
    triggered: Cell<bool>,
    equality_complete: Cell<bool>,
    unfiltered_started: Cell<bool>,
}

impl<'a, 'data> Storage<'a, 'data> {
    pub(super) fn new(data: &'a InsertTransaction<'data>, fault: Fault) -> Self {
        Self {
            data,
            fault,
            triggered: Cell::new(false),
            equality_complete: Cell::new(false),
            unfiltered_started: Cell::new(false),
        }
    }
    pub(super) fn triggered(&self) -> bool {
        self.triggered.get()
    }
    fn active(&self) -> bool {
        match self.fault.activation {
            Activation::Immediate => true,
            Activation::AfterEqualityEnumeration => self.equality_complete.get(),
            Activation::AfterUnfilteredReadStarts => self.unfiltered_started.get(),
        }
    }
}

impl Graph for Storage<'_, '_> {
    fn quads<'a>(
        &'a self,
        s: Option<NamedOrBlankNodeRef<'a>>,
        p: Option<NamedNodeRef<'a>>,
        o: Option<TermRef<'a>>,
    ) -> impl Iterator<Item = Result<Quad>> + 'a {
        let scan = Scan::selectors(s, p, o);
        let equality = scan == Scan::new(None, Some("sameAs"), None);
        if scan == Scan::new(None, None, None) {
            self.unfiltered_started.set(true);
        }
        let selected = scan == self.fault.scan;
        let mut source = self
            .data
            .quads_for_pattern(s, p, o, Some(GraphNameRef::DefaultGraph));
        let mut returned = 0;
        let mut failed = false;
        std::iter::from_fn(move || {
            if failed {
                return None;
            }
            if selected
                && self.active()
                && !self.triggered.get()
                && returned == self.fault.after_results
            {
                self.triggered.set(true);
                failed = true;
                return Some(Err(Error::Runtime(FAILURE.into())));
            }
            let result = source.next();
            if result.is_none() && equality {
                self.equality_complete.set(true);
            }
            result.map(|quad| {
                returned += 1;
                Ok(quad.into_owned())
            })
        })
    }
    fn query(&self, _: PreparedSparqlQuery) -> Result<QueryResults<'_>> {
        panic!("indexed kernels must not evaluate queries")
    }
    fn insert(&mut self, _: Quad) {
        panic!("a kernel published a fact before its round completed")
    }
}

pub(super) fn node(value: &str) -> NamedNode {
    let namespace = match value {
        "sameAs" | "intersectionOf" => "http://www.w3.org/2002/07/owl#",
        "type" | "first" | "rest" | "nil" => "http://www.w3.org/1999/02/22-rdf-syntax-ns#",
        _ => "urn:synthetic:",
    };
    NamedNode::new(format!("{namespace}{value}")).unwrap()
}

pub(super) fn quad(s: &str, p: &str, o: &str) -> Quad {
    Quad::new(node(s), node(p), node(o), GraphName::DefaultGraph)
}
