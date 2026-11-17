//! Pinned OWL inconsistency messages, published only after successful closure.
mod collector;
mod lists;
pub(super) mod rdf_list;
mod relations;
mod restrictions;
mod terms;

pub(super) use collector::Diagnostics;
pub(super) use terms::{cardinality_zero, valid_lexical};
