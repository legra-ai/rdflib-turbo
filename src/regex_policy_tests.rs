//! Host snapshots use this interpreter's Unicode and regex semantics.

use pyo3::prelude::*;
use pyo3::types::PyString;
use pyshacl_turbo_engine::RegexClass;

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
