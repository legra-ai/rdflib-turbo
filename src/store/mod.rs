//! Native storage and transaction handles, owned by this extension only.
mod bindings;
mod changes;
#[cfg(test)]
mod changes_tests;
mod context_read;
mod fork;
mod import;
#[cfg(test)]
mod import_tests;
mod input_terms;
mod native_store;
mod native_store_api;
mod native_store_inference;
mod native_store_reads;
mod nt_format;
#[cfg(test)]
mod nt_output_tests;
mod nt_python_output;
mod operations;
pub(crate) mod output;
mod pattern;
mod prepared;
mod prepared_changes;
mod quad_read;
mod quad_stream;
mod read;
mod read_input;
#[cfg(test)]
mod read_quads_tests;
mod registration;
mod session;
mod shacl;
mod shacl_inference;
mod shacl_read;
mod shacl_rules;
mod term_output;
mod transaction;
mod typed;
mod typed_view;
mod union_read;

pub(crate) use native_store::NativeStore;
pub(crate) use pattern::graph as parse_graph_scope;
pub(crate) use registration::register;
pub(crate) use shacl::PreparedValidation;
pub(crate) use shacl_rules::PreparedRules;
pub(crate) use transaction::NativeTransaction;

#[cfg(test)]
mod session_tests;

#[cfg(all(test, feature = "allocation-profiling"))]
mod read_input_tests;

#[cfg(test)]
mod update_scope_tests;

#[cfg(test)]
mod generalized_binding_tests;
