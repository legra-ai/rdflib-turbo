//! Scoped RDFLib Graph NT formatting without narrowing generalized statement roles.
#[path = "nt_format_document.rs"]
mod document;
#[path = "nt_numeric.rs"]
mod numeric;

pub(super) use document::{GraphNtOutput, dump_graph_nt};

#[cfg(test)]
#[path = "nt_format_tests.rs"]
mod tests;
