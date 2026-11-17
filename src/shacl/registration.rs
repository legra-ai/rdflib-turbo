//! Registration of the SHACL classes and protocol constants.
use pyo3::prelude::*;

use super::dataset_validation;
use super::failure::SharedValidationFailure;
use super::shared_shapes::SharedShapes;

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<SharedShapes>()?;
    module.add_class::<dataset_validation::PreparedDatasetValidation>()?;
    module.add_class::<crate::store::PreparedValidation>()?;
    module.add_class::<crate::store::PreparedRules>()?;
    module.add("SHARED_SHACL_PROTOCOL", 1)?;
    module.add(
        "SHACL_ENGINE_FINGERPRINT",
        pyshacl_turbo_engine::engine_fingerprint(),
    )?;
    module.add(
        "SHACL_ENGINE_SOURCE",
        env!("RDFLIB_TURBO_SHACL_ENGINE_SOURCE"),
    )?;
    module.add(
        "SharedValidationFailure",
        module.py().get_type::<SharedValidationFailure>(),
    )?;
    Ok(())
}
