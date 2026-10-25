//! Interpreter-owned immutable tables and compilation-only lexical leases.
use std::{
    ffi::{CString, c_char, c_int},
    sync::{Arc, Mutex},
};

use pyo3::{
    class::gc::{PyTraverseError, PyVisit},
    exceptions::PyRuntimeError,
    prelude::*,
    types::PyString,
};
use pyshacl_turbo_engine::{Error, RegexLexicalResolver, RegexPolicy};

type GetName = unsafe extern "C" fn(u32, *mut c_char, c_int, c_int) -> c_int;
type GetCode = unsafe extern "C" fn(*const c_char, c_int, *mut u32, c_int) -> c_int;

// Audited CPython 3.11–3.15 pycore_ucnhash.h layout, not a stable-ABI promise.
#[repr(C)]
#[derive(Clone, Copy)]
struct UnicodeNamesApi {
    getname: Option<GetName>,
    getcode: Option<GetCode>,
}

fn read_names(capsule: &Bound<'_, PyAny>) -> PyResult<GetCode> {
    // SAFETY: called attached; CPython checks the capsule type and exact name.
    let pointer = unsafe {
        pyo3::ffi::PyCapsule_GetPointer(capsule.as_ptr(), c"unicodedata._ucnhash_CAPI".as_ptr())
    };
    if pointer.is_null() {
        return Err(PyErr::fetch(capsule.py()));
    }
    // SAFETY: this exact supported CPython capsule exports the two-pointer
    // layout above. The caller keeps the capsule and originating module alive.
    let api = unsafe { *pointer.cast::<UnicodeNamesApi>() };
    if api.getname.is_none() || api.getcode.is_none() {
        return Err(PyRuntimeError::new_err("Incomplete host Unicode name API"));
    }
    Ok(api.getcode.expect("validated callback"))
}

fn interpreter(py: Python<'_>) -> PyResult<i64> {
    // SAFETY: the Python token guarantees an attached current interpreter.
    let identity =
        unsafe { pyo3::ffi::PyInterpreterState_GetID(pyo3::ffi::PyInterpreterState_Get()) };
    if identity < 0 {
        return Err(PyErr::fetch(py));
    }
    Ok(identity)
}

#[pyclass(frozen, weakref, name = "_RegexPolicy")]
pub struct NativeRegexPolicy {
    scalar: Arc<RegexPolicy>,
    unicode_module: Py<PyModule>,
    capsule: Py<PyAny>,
    names: GetCode,
    interpreter: i64,
}

#[pymethods]
impl NativeRegexPolicy {
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.unicode_module)?;
        visit.call(&self.capsule)
    }
}

pub(super) fn install(module: &Bound<'_, PyModule>, scalar: Arc<RegexPolicy>) -> PyResult<()> {
    let py = module.py();
    let version = py.version_info();
    if version.major != 3 || !(11..=15).contains(&version.minor) {
        return Err(PyRuntimeError::new_err(
            "Unsupported host Unicode name API version",
        ));
    }
    let unicode_module = py.import("unicodedata")?;
    let capsule = unicode_module.getattr("_ucnhash_CAPI")?;
    let names = read_names(&capsule)?;
    module.add(
        "_REGEX_POLICY",
        Py::new(
            py,
            NativeRegexPolicy {
                scalar,
                unicode_module: unicode_module.unbind(),
                capsule: capsule.unbind(),
                names,
                interpreter: interpreter(py)?,
            },
        )?,
    )
}

/// Retrieve the immutable scalar allocation owned by this registered module.
pub fn registered_regex_policy(module: &Bound<'_, PyModule>) -> PyResult<Arc<RegexPolicy>> {
    Ok(module
        .getattr("_REGEX_POLICY")?
        .extract::<PyRef<'_, NativeRegexPolicy>>()?
        .scalar
        .clone())
}

struct HostLexicalResolver {
    names: GetCode,
    interpreter: i64,
    failure: Mutex<Option<PyErr>>,
}

impl HostLexicalResolver {
    fn host_failure(&self, error: PyErr) -> Error {
        let mut failure = self
            .failure
            .lock()
            .expect("host regex failure lock poisoned");
        assert!(
            failure.is_none(),
            "compiler continued after a host regex failure"
        );
        *failure = Some(error);
        Error::Runtime("Host regex lexical lookup failed".into())
    }
}

impl RegexLexicalResolver for HostLexicalResolver {
    fn is_identifier(&self, name: &str) -> pyshacl_turbo_engine::Result<bool> {
        Python::attach(|py| {
            if interpreter(py).map_err(|error| self.host_failure(error))? != self.interpreter {
                return Err(self.host_failure(PyRuntimeError::new_err(
                    "Regex policy belongs to another interpreter",
                )));
            }
            let value = PyString::new(py, name);
            // SAFETY: attached, fresh exact Unicode object, public identifier
            // operation used by str.isidentifier. Never called while matching.
            let result = unsafe { pyo3::ffi::PyUnicode_IsIdentifier(value.as_ptr()) };
            if PyErr::occurred(py) {
                return Err(self.host_failure(PyErr::fetch(py)));
            }
            match result {
                0 => Ok(false),
                1 => Ok(true),
                _ => {
                    Err(self
                        .host_failure(PyRuntimeError::new_err("Invalid host identifier status")))
                }
            }
        })
    }

    fn named_character(&self, name: &str) -> pyshacl_turbo_engine::Result<Option<char>> {
        let Ok(name) = CString::new(name) else {
            return Ok(None);
        };
        let length = c_int::try_from(name.as_bytes().len())
            .map_err(|_| Error::Invalid("Unicode character name is too long".into()))?;
        let mut scalar = 0u32;
        // SAFETY: the compilation lease retains the originating capsule/module.
        // Audited getcode paths read immutable tables only and set no exception.
        // The final zero excludes named sequences while retaining scalar aliases.
        match unsafe { (self.names)(name.as_ptr(), length, &mut scalar, 0) } {
            0 => Ok(None),
            1 => char::from_u32(scalar).map(Some).ok_or_else(|| {
                Error::Runtime("Host Unicode name API returned a non-scalar".into())
            }),
            _ => Err(Error::Runtime("Invalid host Unicode name status".into())),
        }
    }
}

/// A temporary compiler authority. The engine borrows its pure native resolver;
/// compiled programs retain neither the resolver nor these Python owner edges.
pub struct RegexCompilationPolicy {
    scalar: Arc<RegexPolicy>,
    resolver: HostLexicalResolver,
    _owner: Py<NativeRegexPolicy>,
}

impl RegexCompilationPolicy {
    pub fn scalar(&self) -> Arc<RegexPolicy> {
        self.scalar.clone()
    }
    pub fn resolver(&self) -> &dyn RegexLexicalResolver {
        &self.resolver
    }

    /// Return the original Python lookup error if the native compiler stopped
    /// at that error. Successful compilation after it is an invariant failure.
    pub fn finish<T>(self, result: pyshacl_turbo_engine::Result<T>) -> PyResult<T> {
        self.finish_with(result, super::error)
    }

    /// Preserve the provider binding's own error mapping for non-host errors.
    pub fn finish_with<T, E>(
        self,
        result: Result<T, E>,
        convert: impl FnOnce(E) -> PyErr,
    ) -> PyResult<T> {
        if let Some(error) = self
            .resolver
            .failure
            .into_inner()
            .expect("host regex failure lock poisoned")
        {
            if result.is_ok() {
                return Err(PyRuntimeError::new_err(
                    "Compiler ignored a host regex lookup failure",
                ));
            }
            return Err(error);
        }
        result.map_err(convert)
    }
}

/// Lease an explicitly supplied typed authority before detaching compilation.
pub fn compilation_policy(
    py: Python<'_>,
    owner: Py<NativeRegexPolicy>,
) -> PyResult<RegexCompilationPolicy> {
    let value = owner.bind(py).get();
    if value.interpreter != interpreter(py)? {
        return Err(PyRuntimeError::new_err(
            "Regex policy belongs to another interpreter",
        ));
    }
    Ok(RegexCompilationPolicy {
        scalar: value.scalar.clone(),
        resolver: HostLexicalResolver {
            names: value.names,
            interpreter: value.interpreter,
            failure: Mutex::new(None),
        },
        _owner: owner,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::{exceptions::PyValueError, types::PyCapsule};

    #[test]
    fn invalid_capsules_and_missing_callbacks_fail_before_lookup() {
        Python::initialize();
        Python::attach(|py| {
            assert!(read_names(PyString::new(py, "not a capsule").as_any()).is_err());
            let wrong = PyCapsule::new_with_value(py, 42usize, c"wrong.name").unwrap();
            assert!(read_names(wrong.as_any()).is_err());
            let empty = PyCapsule::new_with_value(
                py,
                UnicodeNamesApi {
                    getname: None,
                    getcode: None,
                },
                c"unicodedata._ucnhash_CAPI",
            )
            .unwrap();
            let error = read_names(empty.as_any()).unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("Incomplete host Unicode name API")
            );
        });
    }

    #[test]
    fn original_host_failure_is_returned_and_cannot_be_accepted_as_success() {
        Python::initialize();
        Python::attach(|py| {
            let module = PyModule::new(py, "host_failure_test").unwrap();
            crate::register(
                &module,
                crate::provider_tests::test_regex_policy(),
                crate::provider_tests::test_comparison_host(),
            )
            .unwrap();
            let authority = || module.getattr("_REGEX_POLICY").unwrap().extract().unwrap();
            let policy = compilation_policy(py, authority()).unwrap();
            let original = PyValueError::new_err("original host failure");
            let identity = original.value(py).clone().unbind();
            let signal = policy.resolver.host_failure(original);
            let error = policy.finish(Err::<(), _>(signal)).unwrap_err();
            assert!(error.value(py).is(identity.bind(py)));
            let policy = compilation_policy(py, authority()).unwrap();
            let _signal = policy
                .resolver
                .host_failure(PyValueError::new_err("cannot continue"));
            assert!(
                policy
                    .finish(Ok(()))
                    .unwrap_err()
                    .to_string()
                    .contains("Compiler ignored")
            );
        });
    }
}
