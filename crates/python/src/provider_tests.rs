//! Reusable registration retains one compiled allocation inside its extension.

use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};

use super::{PyCompiledShapes, register};

pub(crate) fn test_regex_policy() -> Arc<pyshacl_turbo_engine::RegexPolicy> {
    Arc::new(pyshacl_turbo_engine::RegexPolicy::from_scalars(
        |_| pyshacl_turbo_engine::RegexClasses::new(false, false, false),
        |ch| ch,
        Vec::new(),
        false,
    ))
}

#[test]
fn host_ffi_compilers_require_an_explicit_typed_module_authority() {
    use pyo3::types::PyDict;
    Python::initialize();
    Python::attach(|py| {
        let module = PyModule::new(py, "explicit_compiler_policy_test").unwrap();
        register(&module, test_regex_policy()).unwrap();
        let kwargs = PyDict::new(py);
        kwargs
            .set_item("_policy", module.getattr("_REGEX_POLICY").unwrap())
            .unwrap();
        let class = module.getattr("CompiledShapes").unwrap();
        let args = (
            "",
            "turtle",
            py.None(),
            false,
            true,
            Vec::<(String, String)>::new(),
        );
        let compiled = class.call(args, Some(&kwargs)).unwrap();
        assert_eq!(
            compiled
                .extract::<PyRef<'_, PyCompiledShapes>>()
                .unwrap()
                .shared_program()
                .shape_nodes()
                .count(),
            0
        );
        let missing = class
            .call1((
                "",
                "turtle",
                py.None(),
                false,
                true,
                Vec::<(String, String)>::new(),
            ))
            .unwrap_err();
        assert!(missing.to_string().contains("_policy"));
        let input = module
            .getattr("ShapesInput")
            .unwrap()
            .call1((Vec::<i32>::new(),))
            .unwrap();
        let factory = class.getattr("from_input").unwrap();
        factory
            .call(
                (&input, false, true, Vec::<(String, String)>::new()),
                Some(&kwargs),
            )
            .unwrap();
        assert!(
            factory
                .call1((&input, false, true, Vec::<(String, String)>::new()))
                .unwrap_err()
                .to_string()
                .contains("_policy")
        );
    });
}

#[test]
fn registered_compiler_shares_its_actual_program() {
    Python::initialize();
    Python::attach(|py| {
        let module = PyModule::new(py, "dataset_shacl").unwrap();
        let policy = test_regex_policy();
        register(&module, policy.clone()).unwrap();
        assert!(Arc::ptr_eq(
            &policy,
            &super::registered_regex_policy(&module).unwrap()
        ));
        assert!(register(&module, policy).is_err());
        assert!(
            module
                .getattr("_REGEX_POLICY")
                .unwrap()
                .setattr("changed", true)
                .is_err()
        );
        for name in [
            "CompiledShapes",
            "TargetData",
            "parse_rdf",
            "ShapesInput",
            "additions",
            "infer",
            "inoculate",
            "query",
            "ValidationFailure",
            "ShapeLoadFailure",
            "SHACL_ENGINE_FINGERPRINT",
            "SHAPES_BATCH_SIZE",
        ] {
            assert!(module.hasattr(name).unwrap(), "missing capability: {name}");
        }
        let shape = "<urn:shape> <http://www.w3.org/ns/shacl#targetNode> <urn:s>.";
        let kwargs = PyDict::new(py);
        kwargs
            .set_item("_policy", module.getattr("_REGEX_POLICY").unwrap())
            .unwrap();
        let compiled = module
            .getattr("CompiledShapes")
            .unwrap()
            .call(
                (
                    shape,
                    "nt",
                    py.None(),
                    false,
                    true,
                    Vec::<(String, String)>::new(),
                ),
                Some(&kwargs),
            )
            .unwrap();
        let compiled = compiled.extract::<PyRef<'_, PyCompiledShapes>>().unwrap();
        let shared = compiled.shared_program();
        assert!(Arc::ptr_eq(&compiled.inner, &shared));
        assert_eq!(shared.shape_nodes().count(), 1);
    });
}

#[test]
fn typed_shape_load_failure_retains_two_arguments_without_reclassifying_invalid_input() {
    Python::initialize();
    Python::attach(|py| {
        let failure = super::error(pyshacl_turbo_engine::Error::ShapeLoad {
            message: "shape declaration",
            link: "https://example.org/reference",
        });
        assert!(failure.is_instance_of::<super::ShapeLoadFailure>(py));
        let args: (String, String) = failure
            .value(py)
            .getattr("args")
            .unwrap()
            .extract()
            .unwrap();
        assert_eq!(
            args,
            (
                "shape declaration".into(),
                "https://example.org/reference".into()
            )
        );
        let invalid = super::error(pyshacl_turbo_engine::Error::Invalid("syntax".into()));
        assert!(!invalid.is_instance_of::<super::ShapeLoadFailure>(py));
    });
}

#[test]
fn decimal_scalar_policy_matches_host_and_runs_on_detached_threads() {
    Python::initialize();
    Python::attach(|py| {
        let module = py.import("unicodedata").unwrap();
        for ch in [
            '0',
            '9',
            '٠',
            '١',
            '\u{11bf0}',
            '\u{10d40}',
            '\u{1e5f1}',
            'x',
        ] {
            let expected = module
                .getattr("decimal")
                .unwrap()
                .call1((ch.to_string(), py.None()))
                .unwrap()
                .extract::<Option<u8>>()
                .unwrap();
            assert_eq!(super::python_decimal_digit(ch), expected);
            let actual = py.detach(move || {
                std::thread::spawn(move || super::python_decimal_digit(ch))
                    .join()
                    .unwrap()
            });
            assert_eq!(actual, expected);
        }
    });
}

#[test]
fn numeric_load_signals_preserve_type_and_leave_other_invalid_errors_unchanged() {
    Python::initialize();
    Python::attach(|py| {
        let failure = super::error(pyshacl_turbo_engine::Error::ConstraintLoad {
            message: "constraint declaration",
            link: "https://example.org/constraint",
        });
        assert!(failure.is_instance_of::<super::ConstraintLoadFailure>(py));
        let args = failure
            .value(py)
            .getattr("args")
            .unwrap()
            .extract::<(String, String)>()
            .unwrap();
        assert_eq!(
            args,
            (
                "constraint declaration".into(),
                "https://example.org/constraint".into()
            )
        );
        let invalid = super::error(pyshacl_turbo_engine::Error::Invalid("bad RDF".into()));
        assert!(invalid.is_instance_of::<pyo3::exceptions::PyValueError>(py));
        assert!(!invalid.is_instance_of::<super::ConstraintLoadFailure>(py));
        let conversion = super::error(pyshacl_turbo_engine::Error::TypeError("int(None)"));
        assert!(conversion.is_instance_of::<pyo3::exceptions::PyTypeError>(py));
        assert_eq!(
            conversion.value(py).str().unwrap().to_str().unwrap(),
            "int(None)"
        );
    });
}
