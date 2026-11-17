//! One-step equality compositions over one immutable inference-round snapshot.
#[path = "same_as_components.rs"]
mod components;
#[path = "same_as_dense.rs"]
mod dense;
#[cfg(test)]
#[path = "same_as_difference_tests.rs"]
mod difference_tests;
#[path = "same_as_replacements.rs"]
mod replacements;
#[path = "same_as_rows.rs"]
mod rows;
#[path = "same_as_symmetry.rs"]
mod symmetry;

pub(super) use replacements::apply;
