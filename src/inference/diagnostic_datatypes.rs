//! Diagnostic conversion follows owlrl 7.6.2, separately from RDFLib values.

mod cardinality;
mod comparison;
mod comparison_warning;
pub(crate) use comparison::Host as ComparisonHost;
mod context;
mod decimal;
mod duration_truthiness;
mod integer;
mod lexical;
pub(super) mod list_truthiness;
mod numeric;
mod signals;
mod temporal;
mod text;
mod truthiness;
pub(crate) use shacl_bindings::host_unicode as unicode;

pub(crate) use context::LexicalContext;
pub(crate) use lexical::LexicalFailure;
pub(super) use lexical::{LexicalResult, cardinality_zero, check};
pub(super) use numeric::{PythonNumericValue, parse_numeric};
pub(crate) use signals::SignalPublisher;
pub(super) use truthiness::truthy;

#[cfg(test)]
#[path = "diagnostic_datatypes/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "diagnostic_datatypes/signal_tests.rs"]
mod signal_tests;
