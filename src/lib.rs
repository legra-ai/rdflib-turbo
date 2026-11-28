mod canonical;
mod dataset_inference;
mod dataset_output;
mod dataset_state;
mod errors;
mod graph;
mod inference;
mod native_dataset;
mod python_module;
mod rdf_format;
mod regex_policy;
#[cfg(test)]
mod regex_policy_tests;
mod shacl;
mod store;

use dataset_state::DatasetState;
use errors::{Error, Result, error};
use native_dataset::NativeDataset;
#[cfg(test)]
use python_module::_native;
use rdf_format::format;

#[cfg(test)]
mod native_tests;

#[cfg(all(test, feature = "allocation-profiling"))]
mod allocation_tests;

#[cfg(test)]
mod store_tests;

#[cfg(test)]
mod construct_tests;

#[cfg(test)]
mod dataset_revision_tests;

#[cfg(test)]
mod dataset_generalized_tests;
