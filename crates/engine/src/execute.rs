use std::collections::{HashMap, HashSet};

use oxigraph::model::*;
use oxigraph::sparql::QueryResults;

use crate::compile::{CompiledShapes, Shape, Target};
use crate::constraints::Constraint;
use crate::functions::Functions;
use crate::graph::*;
use crate::graph_write::GraphWrite;
use crate::path::Path;
use crate::{Error, GraphRead, Result, UnionGraph};

#[cfg(test)]
#[path = "execute_tests.rs"]
mod coverage_tests;

#[cfg(test)]
mod report_message_tests;

#[cfg(test)]
#[path = "language_message_tests.rs"]
mod language_message_tests;

#[derive(Clone, Debug)]
pub struct Options {
    pub iterate_rules: bool,
    pub max_rule_iterations: usize,
    pub max_validation_depth: usize,
    pub allow_infos: bool,
    pub allow_warnings: bool,
    pub abort_on_first: bool,
    pub focus_nodes: Vec<Term>,
    pub use_shapes: Vec<Term>,
    pub namespaces: Vec<(String, String)>,
    pub comparison: crate::CoreComparisonPolicy,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            iterate_rules: false,
            max_rule_iterations: 100,
            max_validation_depth: 15,
            allow_infos: false,
            allow_warnings: false,
            abort_on_first: false,
            focus_nodes: Vec::new(),
            use_shapes: Vec::new(),
            namespaces: Vec::new(),
            comparison: crate::CoreComparisonPolicy::default(),
        }
    }
}

pub struct ValidationReport {
    pub conforms: bool,
    pub graph: Dataset,
    pub text: String,
    pub recursion_warnings: Vec<String>,
}

/// A validation report and only the rules that would be newly inserted.
pub struct ValidationOutcome {
    pub report: ValidationReport,
    pub additions: Dataset,
}

pub struct RuleExpansion {
    pub graph: Dataset,
    pub recursion_warnings: Vec<String>,
}

#[derive(Clone, Copy)]
struct EvaluationStep {
    shape: usize,
    component: &'static str,
}

struct Evaluation {
    path_values: HashMap<(Path, Term), Vec<Term>>,
    memberships: HashMap<(Term, Term), bool>,
    class_targets: HashMap<Term, HashSet<Term>>,
    cached_terms: usize,
    functions: Functions,
    path: Vec<EvaluationStep>,
    max_depth: usize,
    warnings: Vec<String>,
    batch: Option<EvaluationBatch>,
    comparison: crate::CoreComparisonPolicy,
}

#[derive(Clone, Copy)]
struct EvaluationBatch {
    shape: usize,
    depth: usize,
    has_values: bool,
}

enum DataDescription {
    Expanded,
    Identity,
}

struct Violation {
    shape: usize,
    focus: Term,
    component: NamedNode,
    message: String,
    value: Option<Term>,
    path: Option<Term>,
    details: Vec<Violation>,
    data_description: DataDescription,
    source_constraint: Option<Term>,
    messages: Option<Vec<Term>>,
    comparison: Option<ComparisonMessage>,
}

enum ComparisonMessage {
    Range {
        operator: crate::ComparisonOperator,
        bound: Term,
    },
    PropertyPair {
        operator: crate::ComparisonOperator,
        predicate: NamedNode,
    },
}

impl Violation {
    fn printed_path(&self) -> Option<&Term> {
        let path = self.path.as_ref()?;
        // Reference report text tests identifier truthiness, while its RDF
        // report still retains the actual empty URI/blank resultPath term.
        match path {
            Term::NamedNode(node) if node.as_str().is_empty() => None,
            Term::BlankNode(node) if node.as_str().is_empty() => None,
            _ => Some(path),
        }
    }

    fn new(
        shape: usize,
        definition: &Shape,
        focus: &Term,
        component: &str,
        value: Option<&Term>,
    ) -> Self {
        Self {
            shape,
            focus: focus.clone(),
            component: iri(SH, component),
            message: String::new(),
            value: value.cloned(),
            path: definition.path_node.clone(),
            details: Vec::new(),
            data_description: DataDescription::Expanded,
            source_constraint: None,
            messages: None,
            comparison: None,
        }
    }

    fn with_comparison(mut self, comparison: ComparisonMessage) -> Self {
        self.comparison = Some(comparison);
        self
    }
}

mod backend;
mod constraint_advanced;
mod constraint_list;
mod constraint_relational;
mod constraint_scalar;
mod constraints;
mod data_list;
mod evaluation;
mod report;
mod rules;
mod targets;
mod validation;

#[cfg(test)]
use evaluation::is_instance;

#[cfg(test)]
mod union_tests;

#[cfg(test)]
mod traversal_tests;

#[cfg(test)]
#[path = "execute_borrowed_tests.rs"]
mod borrowed_tests;
