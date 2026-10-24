//! Standalone and dataset APIs retain the same compiled allocation in one DSO.

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};

use super::SharedShapes;

#[test]
fn shared_validation_reuses_the_registered_standalone_program() {
    Python::initialize();
    Python::attach(|py| {
        let module = PyModule::new(py, "provider_probe").unwrap();
        crate::_native(&module).unwrap();
        let compiler = module
            .getattr("shacl")
            .unwrap()
            .getattr("CompiledShapes")
            .unwrap();
        let source = "<urn:shape> <http://www.w3.org/ns/shacl#targetNode> <urn:s>.";
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
        let compile = || {
            compiler
                .call(
                    (
                        source,
                        "nt",
                        py.None(),
                        false,
                        true,
                        Vec::<(String, String)>::new(),
                    ),
                    Some(&kwargs),
                )
                .unwrap()
        };
        let first = compile();
        let second = compile();
        let first = first
            .extract::<PyRef<'_, shacl_bindings::PyCompiledShapes>>()
            .unwrap();
        let second = second
            .extract::<PyRef<'_, shacl_bindings::PyCompiledShapes>>()
            .unwrap();
        let shared = SharedShapes::from_compiled(&first);
        assert!(shared.shares_program(&first));
        assert!(!shared.shares_program(&second));
    });
}

#[test]
fn provider_uses_the_same_typed_shape_load_failure_as_standalone_bindings() {
    Python::initialize();
    Python::attach(|py| {
        let failure = super::error(pyshacl_turbo_engine::Error::ShapeLoad {
            message: "shape declaration",
            link: "https://example.org/reference",
        });
        assert!(failure.is_instance_of::<shacl_bindings::ShapeLoadFailure>(py));
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
        assert!(!invalid.is_instance_of::<shacl_bindings::ShapeLoadFailure>(py));
    });
}
