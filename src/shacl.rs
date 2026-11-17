//! SHACL executes where its dataset or transaction is owned. No Rust pointer crosses a DSO.
mod settings;
pub(crate) use settings::InferenceStage;
#[cfg(test)]
pub(crate) use settings::Mode as InferenceMode;
use settings::Settings;

mod report;
pub(crate) use report::Report;

mod commit;
mod dataset_validation;
mod expansion;
mod failure;
mod inferred_dataset;
#[cfg(test)]
mod provider_binding_tests;
mod registration;
mod report_terms;
mod shared_shapes;

pub(crate) use failure::error;
pub(crate) use registration::register;
pub(crate) use report_terms::Output;
pub(crate) use shared_shapes::SharedShapes;

#[cfg(test)]
mod read_lock_tests;
