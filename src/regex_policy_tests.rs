//! Host snapshots use this interpreter's Unicode and regex semantics.

use pyo3::prelude::*;
use pyo3::types::PyString;
use pyshacl_turbo_engine::RegexClass;

#[pyfunction]
fn initialize_source_regex_policy(py: Python<'_>) -> PyResult<()> {
    crate::regex_policy::build(py).map(|_| ())
}

#[test]
fn fresh_policy_and_parent_imports_cannot_invert_module_locks() {
    const CHILD: &str = "RDFLIB_TURBO_REGEX_IMPORT_CHILD";
    let case = "regex_policy_tests::fresh_policy_and_parent_imports_cannot_invert_module_locks";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", case, "--nocapture"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "Fresh import regression failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("REGEX_IMPORT_RACE_COMPLETED"));
        return;
    }
    Python::initialize();
    Python::attach(|py| {
        let globals = pyo3::types::PyDict::new(py);
        globals
            .set_item(
                "initialize_policy",
                wrap_pyfunction!(initialize_source_regex_policy, py).unwrap(),
            )
            .unwrap();
        py.run(
            cr#"import sys
import threading
import importlib._bootstrap as bootstrap
assert 're' not in sys.modules, 'This regression requires a fresh parent module'
original = bootstrap._get_module_lock
first_lock = threading.Event()
parent_locked = threading.Event()
policy_first = []
errors = []
class ObservedLock:
    def __init__(self, name, lock):
        self.name, self.lock = name, lock
    def acquire(self):
        self.lock.acquire()
        role = threading.current_thread().name
        if role == 'policy' and self.name.startswith('re') and not policy_first:
            policy_first.append(self.name)
            first_lock.set()
            if self.name == 're._casefix':
                assert parent_locked.wait(10), 'Reference parent import never acquired its lock'
        if role == 'reference' and self.name == 're':
            parent_locked.set()
    def release(self):
        return self.lock.release()
    def __getattr__(self, name):
        return getattr(self.lock, name)
def observed(name):
    return ObservedLock(name, original(name))
def policy():
    try:
        initialize_policy()
    except BaseException as error:
        errors.append(('policy', type(error).__name__, str(error)))
def reference():
    try:
        __import__('re')
    except BaseException as error:
        errors.append(('reference', type(error).__name__, str(error)))
bootstrap._get_module_lock = observed
try:
    a = threading.Thread(target=policy, name='policy', daemon=True)
    a.start()
    assert first_lock.wait(10), 'Policy never started importing its host authority'
    b = threading.Thread(target=reference, name='reference', daemon=True)
    b.start()
    a.join(20)
    b.join(20)
    assert not a.is_alive() and not b.is_alive(), 'Concurrent host imports hung'
    assert not errors, errors
    assert policy_first == ['re'], policy_first
finally:
    bootstrap._get_module_lock = original
print('REGEX_IMPORT_RACE_COMPLETED', flush=True)"#,
            Some(&globals),
            None,
        )
        .unwrap();
    });
}

#[test]
fn host_ffi_store_and_shared_compilers_require_the_module_authority() {
    use pyo3::types::{PyDict, PyModule};
    Python::initialize();
    Python::attach(|py| {
        let module = PyModule::new(py, "explicit_provider_policy_test").unwrap();
        crate::_native(&module).unwrap();
        let kwargs = PyDict::new(py);
        kwargs
            .set_item(
                "_policy",
                module
                    .getattr("shacl")
                    .unwrap()
                    .getattr("_REGEX_POLICY")
                    .unwrap(),
            )
            .unwrap();
        kwargs
            .set_item(
                "_comparison_policy",
                module
                    .getattr("shacl")
                    .unwrap()
                    .getattr("_VALUE_COMPARISON_POLICY")
                    .unwrap(),
            )
            .unwrap();
        let shapes = module.getattr("SharedShapes").unwrap();
        shapes
            .call(("", false, Vec::<(String, String)>::new()), Some(&kwargs))
            .unwrap();
        assert!(
            shapes
                .call1(("", false, Vec::<(String, String)>::new()))
                .unwrap_err()
                .to_string()
                .contains("_policy")
        );
        kwargs.del_item("_comparison_policy").unwrap();
        assert!(
            shapes
                .call(("", false, Vec::<(String, String)>::new()), Some(&kwargs))
                .unwrap_err()
                .to_string()
                .contains("_comparison_policy")
        );
        let store = module.getattr("NativeStore").unwrap().call0().unwrap();
        let compiler = store.getattr("compile_shapes").unwrap();
        compiler
            .call(
                (false, true, Vec::<(String, String)>::new(), py.None()),
                Some(&kwargs),
            )
            .unwrap();
        assert!(
            compiler
                .call1((false, true, Vec::<(String, String)>::new(), py.None()))
                .unwrap_err()
                .to_string()
                .contains("_policy")
        );
    });
}

#[test]
fn host_snapshot_matches_python_classes_lowercase_and_semantic_version() {
    Python::initialize();
    Python::attach(|py| {
        let policy = crate::regex_policy::build(py).unwrap();
        let cases: std::collections::HashMap<u32, Vec<u32>> = py
            .import("re._casefix")
            .unwrap()
            .getattr("_EXTRA_CASES")
            .unwrap()
            .extract()
            .unwrap();
        for (key, mut expected) in cases {
            expected.sort_unstable();
            expected.dedup();
            let expected = expected
                .into_iter()
                .map(|value| char::from_u32(value).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(policy.extra_cases(char::from_u32(key).unwrap()), expected);
        }
        let sre = py.import("_sre").unwrap();
        for ch in [
            '5',
            '_',
            '²',
            'Ⅷ',
            'β',
            '\u{1c89}',
            '\u{1e5f1}',
            '\u{11bf0}',
            '\u{1c}',
            ' ',
            '\u{200b}',
            'İ',
            'ı',
            'ſ',
            'K',
        ] {
            let text = PyString::new(py, &ch.to_string());
            let predicate = |name| text.call_method0(name).unwrap().extract::<bool>().unwrap();
            assert_eq!(
                policy.contains(RegexClass::Digit, ch),
                predicate("isdecimal"),
                "digit {ch:?}"
            );
            assert_eq!(
                policy.contains(RegexClass::Word, ch),
                ch == '_' || predicate("isalnum"),
                "word {ch:?}"
            );
            assert_eq!(
                policy.contains(RegexClass::Space, ch),
                predicate("isspace"),
                "space {ch:?}"
            );
            let expected: u32 = sre
                .getattr("unicode_tolower")
                .unwrap()
                .call1((ch as u32,))
                .unwrap()
                .extract()
                .unwrap();
            assert_eq!(policy.simple_lower(ch) as u32, expected, "lower {ch:?}");
        }
        let expected: bool = py
            .import("re")
            .unwrap()
            .getattr("search")
            .unwrap()
            .call1((r"\B", ""))
            .unwrap()
            .is_none();
        assert_eq!(policy.empty_nonboundary(), !expected);
        assert!(policy.allocated_bytes() < 128 * 1024);
        let original = policy.clone();
        py.detach(move || {
            std::thread::spawn(move || {
                assert!(policy.contains(RegexClass::Word, 'β'));
                assert_eq!(policy.simple_lower('İ'), 'i');
            })
            .join()
            .unwrap();
        });
        assert_eq!(original.simple_lower('K'), 'k');
    });
}
