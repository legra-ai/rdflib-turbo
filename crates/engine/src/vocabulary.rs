//! Drop-in handling of the `sh:` vocabulary in a shapes graph.
//!
//! The reference validator (pyshacl) only looks up the terms it executes and
//! treats every other `sh:` predicate or type as an annotation. This program
//! does the same, so newer or misspelled SHACL vocabulary never refuses a
//! shapes graph the reference validates. The exceptions are features the
//! reference can execute but this engine cannot: those fail explicitly
//! instead of silently diverging from the reference result.
use std::collections::HashSet;

use oxigraph::model::*;

use crate::graph::{RDF, SH, iri};
use crate::{Error, GraphRead, Result};

/// SHACL-JS predicates, executed by the reference when JavaScript is enabled.
const JAVASCRIPT_PREDICATES: &[&str] = &["js", "jsFunctionName", "jsLibrary", "jsLibraryURL"];

/// SHACL-JS classes, executed by the reference when JavaScript is enabled.
const JAVASCRIPT_TYPES: &[&str] = &[
    "JSConstraint",
    "JSConstraintComponent",
    "JSExecutable",
    "JSFunction",
    "JSLibrary",
    "JSRule",
    "JSTarget",
    "JSTargetType",
    "JSValidator",
];

/// Advanced-mode classes the reference registers globally but this engine
/// does not implement (a bare `sh:SHACLFunction` is not a `sh:SPARQLFunction`).
const UNSUPPORTED_ADVANCED_TYPES: &[&str] = &["SHACLFunction"];

/// The executable resources of one shapes program and its mode.
pub(crate) struct VocabularyPolicy<'a> {
    advanced: bool,
    executable_nodes: &'a HashSet<Term>,
}

impl<'a> VocabularyPolicy<'a> {
    pub(crate) fn new(advanced: bool, executable_nodes: &'a HashSet<Term>) -> Self {
        Self {
            advanced,
            executable_nodes,
        }
    }

    /// Refuse only the executable vocabulary this engine cannot run; ignore
    /// every other `sh:` term as the reference does.
    pub(crate) fn check(&self, store: &(impl GraphRead + ?Sized)) -> Result<()> {
        let rdf_type = Term::from(iri(RDF, "type"));
        for quad in store.matching_quads(None, None, None, Some(GraphNameRef::DefaultGraph)) {
            let quad = quad?;
            if quad.predicate == rdf_type {
                if let Some(name) = Self::shacl_name(&quad.object)
                    && self.refuses_type(name)
                {
                    return Err(Error::Unsupported(format!("sh:{name}")));
                }
                continue;
            }
            // A shapes graph may also contain ordinary data and prior reports.
            // Only executable resources can carry an executed predicate.
            if self.executable_nodes.contains(&quad.subject)
                && let Some(name) = Self::shacl_name(&quad.predicate)
                && JAVASCRIPT_PREDICATES.contains(&name)
            {
                return Err(Error::Unsupported(format!("sh:{name} on {}", quad.subject)));
            }
        }
        Ok(())
    }

    fn refuses_type(&self, name: &str) -> bool {
        JAVASCRIPT_TYPES.contains(&name)
            || (self.advanced && UNSUPPORTED_ADVANCED_TYPES.contains(&name))
    }

    fn shacl_name(term: &Term) -> Option<&str> {
        match term {
            Term::NamedNode(node) => node.as_str().strip_prefix(SH),
            _ => None,
        }
    }
}
