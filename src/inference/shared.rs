//! Lower portable rule definitions into backend-owned compiled queries.

#[cfg(test)]
#[path = "shared_cax_tests.rs"]
mod cax_tests;
#[path = "shared_compiled_rule.rs"]
mod compiled_rule;
#[path = "shared_literal_values.rs"]
mod literal_values;
#[cfg(test)]
#[path = "shared_noop_tests.rs"]
mod noop_tests;
#[path = "shared_noops.rs"]
mod noops;

pub(super) use compiled_rule::CompiledRule;
