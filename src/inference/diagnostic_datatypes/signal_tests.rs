use super::*;
use pyo3::prelude::*;

#[test]
fn caller_flags_publish_after_detached_conversion_even_when_later_work_fails() {
    Python::initialize();
    Python::attach(|py| {
        let decimal = py.import("decimal").unwrap();
        let scope = decimal.call_method0("localcontext").unwrap();
        let caller = scope.call_method0("__enter__").unwrap();
        caller.setattr("prec", 3).unwrap();
        caller.call_method0("clear_flags").unwrap();
        let flags = caller.getattr("flags").unwrap();
        let preexisting = decimal.getattr("FloatOperation").unwrap();
        flags.set_item(&preexisting, true).unwrap();
        let (context, publisher) = LexicalContext::capture_with_publisher(py).unwrap();
        let result: crate::Result<()> = py.detach(move || {
            let literal = oxigraph::model::Literal::new_typed_literal(
                "1.234567",
                oxigraph::model::NamedNode::new_unchecked(
                    "http://www.w3.org/2001/XMLSchema#double",
                ),
            );
            assert_eq!(check(&literal, &context), LexicalResult::Valid);
            Err(crate::Error::Runtime("later native storage failure".into()))
        });
        // Detached work must not touch the Python mapping itself.
        assert!(
            !flags
                .get_item(decimal.getattr("Inexact").unwrap())
                .unwrap()
                .is_truthy()
                .unwrap()
        );
        let error = publisher.complete(py, result).unwrap_err();
        assert!(error.to_string().contains("later native storage failure"));
        for name in ["Inexact", "Rounded", "FloatOperation"] {
            assert!(
                flags
                    .get_item(decimal.getattr(name).unwrap())
                    .unwrap()
                    .is_truthy()
                    .unwrap()
            );
        }
        assert!(decimal.call_method0("getcontext").unwrap().is(&caller));
        assert_eq!(
            caller.getattr("prec").unwrap().extract::<usize>().unwrap(),
            3
        );
        scope
            .call_method1("__exit__", (py.None(), py.None(), py.None()))
            .unwrap();
    });
}

#[test]
fn default_cardinality_signaling_nan_raises_invalid_status_with_and_without_trap() {
    Python::initialize();
    Python::attach(|py| {
        let decimal = py.import("decimal").unwrap();
        let scope = decimal.call_method0("localcontext").unwrap();
        let caller = scope.call_method0("__enter__").unwrap();
        let invalid = decimal.getattr("InvalidOperation").unwrap();
        for trapped in [true, false] {
            caller.call_method0("clear_flags").unwrap();
            caller
                .getattr("traps")
                .unwrap()
                .set_item(&invalid, trapped)
                .unwrap();
            let (context, publisher) = LexicalContext::capture_with_publisher(py).unwrap();
            let result = py.detach(move || {
                let literal = oxigraph::model::Literal::new_typed_literal(
                    "sNaN",
                    oxigraph::model::NamedNode::new_unchecked(
                        "http://www.w3.org/2001/XMLSchema#decimal",
                    ),
                );
                cardinality_zero(&literal, &context).map_err(crate::Error::Lexical)
            });
            let result = publisher.complete(py, result);
            assert_eq!(result.is_err(), trapped);
            assert!(
                caller
                    .getattr("flags")
                    .unwrap()
                    .get_item(&invalid)
                    .unwrap()
                    .is_truthy()
                    .unwrap()
            );
        }
        scope
            .call_method1("__exit__", (py.None(), py.None(), py.None()))
            .unwrap();
    });
}
