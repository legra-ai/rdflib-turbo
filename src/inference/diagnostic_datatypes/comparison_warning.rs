//! Snapshot the one Core NotImplemented warning profile without emitting a warning.
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use pyo3::exceptions::{PyDeprecationWarning, PyRuntimeError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString, PyTuple, PyType};
use pyshacl_turbo_engine::ComparisonFailure;

const MESSAGE: &str = "NotImplemented should not be used in a boolean context";
const MODULE: &str = "rdflib.term";

#[derive(Clone, Debug)]
pub(super) struct Site {
    filename: String,
    line: u32,
}
impl Site {
    pub(super) fn read(py: Python<'_>) -> PyResult<Self> {
        let function = py.import(MODULE)?.getattr("Literal")?.getattr("__le__")?;
        let code = function.getattr("__code__")?;
        let filename = code.getattr("co_filename")?.extract()?;
        for instruction in py
            .import("dis")?
            .call_method1("get_instructions", (function,))?
            .try_iter()?
        {
            let instruction = instruction?;
            let name: String = instruction.getattr("opname")?.extract()?;
            if name.starts_with("POP_JUMP") && name.ends_with("IF_FALSE") {
                let line = instruction
                    .getattr("positions")?
                    .getattr("lineno")?
                    .extract()?;
                return Ok(Self { filename, line });
            }
        }
        Err(PyRuntimeError::new_err(
            "Cannot locate RDFLib Literal.__le__ NotImplemented truth site",
        ))
    }
}
#[derive(Clone, Copy, Debug)]
enum Action {
    Ignore,
    Error,
    Always,
    Once,
}
#[derive(Clone, Debug)]
pub(super) struct Profile {
    truth_is_error: bool,
    action: Action,
    pending: Arc<AtomicUsize>,
}
impl Profile {
    pub(super) fn consume(&self) -> Result<bool, ComparisonFailure> {
        if self.truth_is_error {
            return Err(ComparisonFailure::NotImplementedTruth);
        }
        match self.action {
            Action::Error => return Err(ComparisonFailure::DeprecationWarning),
            Action::Always => {
                self.pending
                    .try_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
                        count.checked_add(1)
                    })
                    .expect("Core warning occurrence count exhausted");
            }
            Action::Once => {
                self.pending.store(1, Ordering::Relaxed);
            }
            Action::Ignore => {}
        }
        Ok(true)
    }
}
pub(super) struct Publisher {
    site: Site,
    pending: Arc<AtomicUsize>,
    warnings: Py<PyModule>,
    term: Py<PyModule>,
}
impl Publisher {
    pub(super) fn refresh_profile(&self, py: Python<'_>) -> PyResult<Profile> {
        profile(py, &self.site, self.warnings.bind(py), self.pending.clone())
    }

    pub(super) fn publish(&self, py: Python<'_>) -> PyResult<()> {
        let count = self.pending.load(Ordering::Relaxed);
        if count == 0 {
            return Ok(());
        }
        let term = self.term.bind(py);
        let registry = if term.hasattr("__warningregistry__")? {
            term.getattr("__warningregistry__")?.cast_into::<PyDict>()?
        } else {
            let registry = PyDict::new(py);
            term.setattr("__warningregistry__", &registry)?;
            registry
        };
        let kwargs = PyDict::new(py);
        kwargs.set_item("module", MODULE)?;
        kwargs.set_item("registry", registry)?;
        for _ in 0..count {
            self.warnings.bind(py).call_method(
                "warn_explicit",
                (
                    MESSAGE,
                    py.get_type::<PyDeprecationWarning>(),
                    &self.site.filename,
                    self.site.line,
                ),
                Some(&kwargs),
            )?;
        }
        Ok(())
    }
}
fn matches_filter(filter: &Bound<'_, PyAny>, text: &str) -> PyResult<bool> {
    if filter.is_none() {
        return Ok(true);
    }
    // CPython _warnings.check_matched treats its internal exact Unicode default
    // filters as literal equality, and compiled regex filters through match().
    if filter.get_type().is(filter.py().get_type::<PyString>()) {
        return Ok(filter.extract::<String>()? == text);
    }
    filter.call_method1("match", (text,))?.is_truthy()
}
fn read_action(name: &str) -> PyResult<Action> {
    match name {
        "ignore" => Ok(Action::Ignore),
        "error" => Ok(Action::Error),
        "always" => Ok(Action::Always),
        "default" | "once" | "module" => Ok(Action::Once),
        _ => Err(PyRuntimeError::new_err(
            "Unsupported Core warning filter action",
        )),
    }
}
fn profile(
    py: Python<'_>,
    site: &Site,
    warnings: &Bound<'_, PyModule>,
    pending: Arc<AtomicUsize>,
) -> PyResult<Profile> {
    let mut action = read_action(&warnings.getattr("defaultaction")?.extract::<String>()?)?;
    if py.version_info().minor < 14 {
        for filter in warnings.getattr("filters")?.try_iter()? {
            let filter = filter?.cast_into::<PyTuple>()?;
            let message = filter.get_item(1)?;
            if !matches_filter(&message, MESSAGE)? {
                continue;
            }
            let category = filter.get_item(2)?.cast_into::<PyType>()?;
            if !py
                .get_type::<PyDeprecationWarning>()
                .is_subclass(&category)?
            {
                continue;
            }
            let module = filter.get_item(3)?;
            if !matches_filter(&module, MODULE)? {
                continue;
            }
            let line: u32 = filter.get_item(4)?.extract()?;
            if line != 0 && line != site.line {
                continue;
            }
            let name: String = filter.get_item(0)?.extract()?;
            action = read_action(&name)?;
            break;
        }
    }
    Ok(Profile {
        truth_is_error: py.version_info().minor >= 14,
        action,
        pending,
    })
}

pub(super) fn capture(py: Python<'_>, site: &Site) -> PyResult<(Profile, Publisher)> {
    let warnings = py.import("warnings")?;
    let pending = Arc::new(AtomicUsize::new(0));
    let profile = profile(py, site, &warnings, pending.clone())?;
    Ok((
        profile,
        Publisher {
            site: site.clone(),
            pending,
            warnings: warnings.unbind(),
            term: py.import(MODULE)?.unbind(),
        },
    ))
}
