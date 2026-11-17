mod axioms;
mod backend;
mod borrowed;
mod cleanup;
#[cfg(test)]
mod cleanup_failure_tests;
pub(crate) use borrowed::BorrowedInference;
pub(crate) use diagnostic_datatypes::{ComparisonHost, SignalPublisher};
pub(crate) use outcome::bind_error_namespace;
mod delta;
mod diagnostic_datatypes;
pub(crate) use diagnostic_datatypes::unicode as python_unicode;
mod diagnostics;
#[cfg(all(test, feature = "allocation-profiling"))]
mod frontier_attribution_tests;
mod intersections;
mod kernel;
mod literal_values;
mod modes;
mod options;
mod outcome;
mod pending;
#[cfg(test)]
mod pending_retirement_tests;
mod request;
mod rules;
mod same_as;
#[cfg(test)]
mod same_as_reference_tests;
mod scheduling;
pub(crate) mod scoped;
mod semantic_delta;
#[cfg(test)]
mod semantic_delta_tests;
mod shared;
#[cfg(test)]
mod shared_group_tests;
#[cfg(test)]
mod shared_rule_tests;
mod staged;
#[cfg(test)]
mod staged_tests;
mod standard_numeric;
mod traversal;

pub(crate) use delta::expand_delta;
pub(crate) use diagnostic_datatypes::LexicalFailure;
pub(crate) use options::InferenceOptions;
use options::expand_configured;
pub(crate) use outcome::InferenceOutcome;
pub(crate) use request::InferenceRequest;
pub(crate) use semantic_delta::InferenceDelta;

#[cfg(test)]
mod diagnostic_tests;
#[cfg(test)]
mod failure_tests;
#[cfg(test)]
mod intersection_tests;
#[cfg(test)]
mod literal_resource_tests;
#[cfg(test)]
mod literal_value_tests;
#[cfg(test)]
mod same_as_tests;

use backend::Graph;
use kernel::expand_inner;
pub(crate) use modes::{expand, expand_into, expand_transaction, supports_mode};
use pending::Pending;
use rules::{HeadDisposition, OWL, apply_rules};
#[cfg(test)]
use rules::{OWL_RULES, PREFIXES};

#[cfg(test)]
mod insertion_failure_tests;

#[cfg(test)]
mod borrowed_cleanup_tests;
#[cfg(test)]
mod borrowed_tests;

#[cfg(test)]
mod scheduling_tests;

#[cfg(test)]
mod generalized_parity_tests;

#[cfg(test)]
mod standard_numeric_tests;
