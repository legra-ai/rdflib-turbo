//! Compilation-only host lexical lookups keep their actual module alive.

use pyo3::{prelude::*, types::PyModule};

#[test]
fn lexical_policy_matches_actual_host_and_survives_module_owner_removal_off_thread() {
    Python::initialize();
    Python::attach(|py| {
        let module = PyModule::new(py, "host_lexical_policy_tests").unwrap();
        crate::register(
            &module,
            crate::provider_tests::test_regex_policy(),
            crate::provider_tests::test_comparison_host(),
        )
        .unwrap();
        let policy = crate::compilation_policy(
            py,
            module.getattr("_REGEX_POLICY").unwrap().extract().unwrap(),
        )
        .unwrap();
        for name in [
            "a",
            "_",
            "class",
            "",
            "0bad",
            "a-b",
            "β",
            "\u{1c89}",
            "a\u{1e5f1}",
        ] {
            let expected: bool = pyo3::types::PyString::new(py, name)
                .call_method0("isidentifier")
                .unwrap()
                .extract()
                .unwrap();
            assert_eq!(policy.resolver().is_identifier(name).unwrap(), expected);
        }
        let database = py.import("unicodedata").unwrap();
        let names = [
            "LATIN SMALL LETTER A",
            "NUL",
            "HANGUL SYLLABLE GA",
            "CJK UNIFIED IDEOGRAPH-4E00",
            "OL ONAL DIGIT ZERO",
            "KEYCAP DIGIT ONE",
            "unknown character",
        ];
        let mut expected = Vec::new();
        for name in names {
            let value = database
                .getattr("lookup")
                .unwrap()
                .call1((name,))
                .ok()
                .map(|value| value.extract::<String>().unwrap());
            let scalar = value.and_then(|text| {
                let mut chars = text.chars();
                let first = chars.next();
                if chars.next().is_none() { first } else { None }
            });
            assert_eq!(policy.resolver().named_character(name).unwrap(), scalar);
            expected.push((name, scalar));
        }
        let weak = py
            .import("weakref")
            .unwrap()
            .getattr("ref")
            .unwrap()
            .call1((module.getattr("_REGEX_POLICY").unwrap(),))
            .unwrap();
        module.delattr("_REGEX_POLICY").unwrap();
        drop(module);
        assert!(!weak.call0().unwrap().is_none());
        py.detach(move || {
            std::thread::spawn(move || {
                for (name, expected) in expected {
                    assert_eq!(policy.resolver().named_character(name).unwrap(), expected);
                }
                assert!(policy.resolver().is_identifier("β").unwrap());
                policy.finish(Ok(())).unwrap();
            })
            .join()
            .unwrap();
        });
        assert!(weak.call0().unwrap().is_none());
    });
}
