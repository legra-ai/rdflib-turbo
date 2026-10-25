//! Register extension-owned storage classes.

use pyo3::prelude::*;

use super::{NativeStore, NativeTransaction, input_terms, term_output};

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<term_output::NativeTermFactories>()?;
    module.add_class::<term_output::NativeReadCursor>()?;
    module.add_class::<super::quad_read::NativeQuadCursor>()?;
    module.add_class::<super::context_read::NativeContextCursor>()?;
    module.add_class::<super::union_read::NativeUnionCursor>()?;
    module.add_class::<NativeStore>()?;
    module.add_class::<super::changes::NativeInferenceChanges>()?;
    module.add_class::<super::prepared_changes::NativePreparedInferenceChanges>()?;
    module.add_class::<input_terms::NativeInputTerms>()?;
    module.add_class::<NativeTransaction>()?;
    Ok(())
}
