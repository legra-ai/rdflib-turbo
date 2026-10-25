//! Snapshot this interpreter's scalar tables once when its module initializes.
use std::{collections::HashMap, sync::Arc};

use pyo3::{exceptions::PyValueError, prelude::*};
use pyshacl_turbo_engine::{RegexClasses, RegexPolicy};

use crate::inference::python_unicode;

pub(crate) fn build(py: Python<'_>) -> PyResult<Arc<RegexPolicy>> {
    // Initialize the public parent before taking a private child import lock.
    // A concurrent ordinary `import re` otherwise takes these locks in the
    // opposite order while importing _compiler/_casefix.
    py.import("re")?;
    // This finite CPython compiler data is copied while attached. Native
    // compilation and matching never call Python's compiler or matcher.
    let cases: HashMap<u32, Vec<u32>> = py
        .import("re._casefix")?
        .getattr("_EXTRA_CASES")?
        .extract()?;
    let scalar = |value| {
        char::from_u32(value).ok_or_else(|| PyValueError::new_err("Invalid host regex case scalar"))
    };
    let extras = cases
        .into_iter()
        .map(|(key, values)| {
            Ok((
                scalar(key)?,
                values
                    .into_iter()
                    .map(scalar)
                    .collect::<PyResult<Vec<_>>>()?,
            ))
        })
        .collect::<PyResult<Vec<_>>>()?;
    let empty_nonboundary = py.version_info().minor >= 14;
    Ok(Arc::new(py.detach(|| {
        RegexPolicy::from_scalars(
            |ch| {
                RegexClasses::new(
                    python_unicode::decimal(ch).is_some(),
                    python_unicode::word(ch),
                    python_unicode::whitespace(ch),
                )
            },
            python_unicode::simple_lower,
            extras,
            empty_nonboundary,
        )
    })))
}
