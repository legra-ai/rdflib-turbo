//! SHACL engine. Unsupported SHACL features are explicit errors.
//! Python is not involved in compilation, targeting, rules, or validation.
//!
//! The diagram and example below describe the borrowed graph query foundation.
#![doc = concat!("<div>\n", include_str!("architecture.svg"), "\n</div>\n\n")]
#![doc = include_str!("architecture.md")]

mod compile;
mod components;
mod constraints;
mod display;
mod execute;
mod expression;
mod functions;
mod graph;
#[cfg(test)]
mod graph_backend_tests;
mod graph_read;
mod graph_views;
#[cfg(test)]
mod graph_views_tests;
mod graph_write;
mod inference;
mod model;
mod ontology;
mod path;
mod pattern;
mod query_graph;
#[cfg(test)]
mod query_graph_tests;
mod rdf11_store;
mod regex_policy;
mod revision;
mod rule_delta;
mod shapes_policy;
mod shapes_source;
mod sparql;
mod sparql_regex;
mod structural;
mod value;

#[cfg(test)]
mod structural_tests;

pub use compile::CompiledShapes;
pub use execute::{Options, RuleExpansion, ValidationOutcome, ValidationReport};
pub use functions::evaluate_query;
pub use graph::{
    parse as parse_graph, parse_dataset, parse_store_snapshot, serialize as serialize_graph,
    store_quad,
};
pub use graph_read::GraphRead;
pub use graph_views::{OverlayGraph, ScopedGraph, UnionGraph};
pub use inference::{expand as infer, expand_into as infer_into};
pub use ontology::inoculate;
pub use oxigraph;
pub use query_graph::QueryGraph;
pub use regex_policy::{RegexClass, RegexClasses, RegexLexicalResolver, RegexPolicy};
pub use revision::fingerprint as engine_fingerprint;
pub use shapes_source::IndexedShapesSource;
pub use value::comparison::{
    ComparisonFailure, ComparisonOperator, ComparisonRole, ComparisonSignal, CoreComparisonPolicy,
    FiniteNumber, NumericComparisonContext, NumericKind, NumericOperand, NumericValue,
    ValueComparisonPolicy, compare_numeric, numeric_kind,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Comparison(#[from] ComparisonFailure),
    #[error("Selected source operation interrupted")]
    SelectedSourceInterrupted,
    #[error("SHACL validation failure: {0}")]
    ValidationFailure(String),
    #[error("{message}\nFor reference, see {link}")]
    ShapeLoad {
        message: &'static str,
        link: &'static str,
    },
    #[error("{message}\nFor reference, see {link}")]
    ConstraintLoad {
        message: &'static str,
        link: &'static str,
    },
    #[error("{0}")]
    TypeError(&'static str),
    #[error("Unsupported feature: {0}")]
    Unsupported(String),
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Runtime(String),
    #[error("SPARQL evaluation failed: {0}")]
    Query(#[from] oxigraph::sparql::QueryEvaluationError),
}

pub type Result<T> = std::result::Result<T, Error>;

mod boolean_state;
pub use boolean_state::BuiltinBooleanState;
mod observed_values;

mod selected_focus_frame;
pub use selected_focus_frame::{
    SelectedFocusBatch, SelectedFocusFrame, SelectedFocusSlot, SelectedFocusSource,
    SelectedSubjectRef,
};
mod selected_focus;
pub use selected_focus::SelectedTargetsRef;

mod selected_values;
pub use selected_values::{
    BooleanObservation, ReportTermRole, SelectedLiteral, SelectedLiteralOwner, SelectedPathRef,
    SelectedSlot, SelectedTerm, SelectedValueFrame, SelectedValueSource,
};
