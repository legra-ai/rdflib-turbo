//! Default canonical numeric Core comparison, separate from diagnostic conversions.
mod host;
mod policy;
mod source;

pub(crate) use host::Host;

#[cfg(test)]
#[path = "comparison_tests.rs"]
mod tests;
