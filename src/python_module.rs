//! The `_native` Python extension module.
use pyo3::prelude::*;

use crate::{NativeDataset, inference, regex_policy, shacl, store};

#[pymodule]
pub(crate) fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let standalone_shacl = PyModule::new(module.py(), "shacl")?;
    shacl_bindings::register(
        &standalone_shacl,
        regex_policy::build(module.py())?,
        std::sync::Arc::new(inference::ComparisonHost::new(module.py())?),
    )?;
    module.add_submodule(&standalone_shacl)?;
    module.add_class::<NativeDataset>()?;
    module.add_class::<inference::InferenceOutcome>()?;
    shacl::register(module)?;
    store::register(module)?;
    Ok(())
}
