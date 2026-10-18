//! Committed read filters borrow caller-pinned Python text during native lookup.
use pyo3::prelude::*;
use pyo3::types::PyString;

use super::{NativeStore, term_output::NativeTermFactories};

#[cfg(feature = "allocation-profiling")]
#[test]
#[ignore = "global allocation counters require this test to run alone with --test-threads=1"]
fn native_read_filter_binding_does_not_copy_large_input_lexical() {
    Python::initialize();
    Python::attach(|py| {
        let native = NativeStore::new(py, None).unwrap();
        // bounded: one deliberately large input field; setup is outside allocation accounting.
        let value = "x".repeat(4 * 1024 * 1024);
        native
            .insert_terms(
                py,
                ("uri".into(), "urn:s".into(), None, None),
                ("uri".into(), "urn:p".into(), None, None),
                ("literal".into(), value.clone(), None, None),
                None,
            )
            .unwrap();
        let lexical = PyString::new(py, &value);
        // Prepare CPython's UTF-8 view before tracking requested Rust heap bytes.
        assert_eq!(lexical.to_str().unwrap().len(), value.len());
        let pattern = (
            ("uri", "urn:s", None::<&str>, None::<&str>),
            py.None(),
            ("literal", lexical, None::<&str>, None::<&str>),
        )
            .into_pyobject(py)
            .unwrap();
        let string = py.get_type::<PyString>().into_any().unbind();
        let factories = Py::new(
            py,
            NativeTermFactories::new(
                py,
                string.clone_ref(py),
                string.clone_ref(py),
                string.clone_ref(py),
                string,
                false,
            )
            .unwrap(),
        )
        .unwrap();
        let arguments = (pattern, py.None(), 0, factories)
            .into_pyobject(py)
            .unwrap();
        let native = Py::new(py, native).unwrap();
        let (result, bytes) = crate::allocation_tests::measure_allocated(|| {
            native
                .bind(py)
                .call_method1("read_pattern", arguments)
                .unwrap()
        });
        assert_eq!(result.get_item(0).unwrap().len().unwrap(), 1);
        assert!(result.get_item(1).unwrap().is_none());
        assert!(
            bytes < 512 * 1024,
            "Read binding copied caller input lexical: {bytes} Rust heap bytes requested"
        );
    });
}

#[cfg(feature = "allocation-profiling")]
#[test]
#[ignore = "global allocation counters require this test to run alone with --test-threads=1"]
fn native_boolean_membership_avoids_result_rows() {
    Python::initialize();
    Python::attach(|py| {
        let native = NativeStore::new(py, None).unwrap();
        // bounded: one large field tests input borrowing; all setup precedes accounting.
        let value = "x".repeat(4 * 1024 * 1024);
        native
            .insert_terms(
                py,
                ("uri".into(), "urn:s".into(), None, None),
                ("uri".into(), "urn:p".into(), None, None),
                ("literal".into(), value.clone(), None, None),
                None,
            )
            .unwrap();
        let lexical = PyString::new(py, &value);
        assert_eq!(lexical.to_str().unwrap().len(), value.len());
        let native = Py::new(py, native).unwrap();
        for (subject, expected) in [("urn:s", true), ("urn:missing", false)] {
            let arguments = (
                ("uri", subject, None::<&str>, None::<&str>),
                ("uri", "urn:p", None::<&str>, None::<&str>),
                ("literal", lexical.clone(), None::<&str>, None::<&str>),
                py.None(),
            )
                .into_pyobject(py)
                .unwrap();
            let (result, bytes) = crate::allocation_tests::measure_allocated(|| {
                native
                    .bind(py)
                    .call_method1("contains_terms", arguments)
                    .unwrap()
            });
            assert!(result.is_instance_of::<pyo3::types::PyBool>());
            assert_eq!(result.extract::<bool>().unwrap(), expected);
            // A present Row allocation alone would exceed this budget. Small input
            // validation/encoding allocations remain allowed; zero total is not assumed.
            assert!(
                bytes < size_of::<super::read::Row>(),
                "Boolean membership allocated a row or copied lexical input: {bytes} bytes"
            );
        }
    });
}
