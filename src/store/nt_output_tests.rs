//! Exact private str/bytes output contracts on committed and pending owners.
use std::sync::{Arc, Mutex};

use oxigraph::model::{GraphName, Literal, NamedNode, Quad};
use oxigraph::store::Store;
use pyo3::ffi::c_str;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyString};

use super::operations::{Operation, Reply};
use super::session::{Control, Session};
use super::typed::QuadInput;
use super::{NativeStore, NativeTransaction};

#[test]
fn committed_nt_bytes_and_default_text_preserve_identical_unicode() {
    let store = Store::new().unwrap();
    store
        .insert(&Quad::new(
            NamedNode::new("urn:s").unwrap(),
            NamedNode::new("urn:p").unwrap(),
            Literal::new_simple_literal("漢字🌍"),
            GraphName::DefaultGraph,
        ))
        .unwrap();
    let native = NativeStore {
        control: Arc::new(Mutex::new(Control::new(store))),
    };
    Python::initialize();
    Python::attach(|py| {
        let owner = Py::new(py, native).unwrap();
        let text = owner
            .bind(py)
            .call_method1("_dump_graph_nt", (py.None(),))
            .unwrap();
        let bytes = owner
            .bind(py)
            .call_method1("_dump_graph_nt", (py.None(), true))
            .unwrap();
        let text = text.cast::<PyString>().unwrap().to_str().unwrap();
        let bytes = bytes.cast::<PyBytes>().unwrap().as_bytes();
        assert_eq!(text, "<urn:s> <urn:p> \"漢字🌍\" .\n");
        assert_eq!(bytes, text.as_bytes());
    });
}

#[test]
fn pending_nt_bytes_keep_transaction_visibility_and_default_text_contract() {
    let store = Store::new().unwrap();
    let control = Arc::new(Mutex::new(Control::new(store.clone())));
    let session = Arc::new(Session::start(control).unwrap());
    session
        .execute(Operation::Update(
            "INSERT DATA { <urn:s> <urn:p> \"漢字🌍\" }".into(),
            None,
        ))
        .unwrap();
    Python::initialize();
    Python::attach(|py| {
        let owner = Py::new(
            py,
            NativeTransaction {
                session: session.clone(),
            },
        )
        .unwrap();
        let text = owner
            .bind(py)
            .call_method1("_dump_graph_nt", (py.None(),))
            .unwrap();
        let bytes = owner
            .bind(py)
            .call_method1("_dump_graph_nt", (py.None(), true))
            .unwrap();
        assert_eq!(
            bytes.cast::<PyBytes>().unwrap().as_bytes(),
            text.cast::<PyString>()
                .unwrap()
                .to_str()
                .unwrap()
                .as_bytes()
        );
        assert!(store.is_empty().unwrap());
    });
    session.finish(false).unwrap();
    assert!(store.is_empty().unwrap());
}

#[test]
fn nt_uri_rejection_is_plain_python_exception_and_keeps_pending_owner_usable() {
    let store = Store::new().unwrap();
    let control = Arc::new(Mutex::new(Control::new(store.clone())));
    let session = Arc::new(Session::start(control).unwrap());
    let iri = |value: &str| ("uri".into(), value.into(), None, None);
    session
        .execute(Operation::InsertTerms(Box::new(QuadInput::new(
            iri("urn:s"),
            iri("urn:p"),
            iri("urn:bad space"),
            None,
        ))))
        .unwrap();
    Python::initialize();
    Python::attach(|py| {
        let owner = Py::new(
            py,
            NativeTransaction {
                session: session.clone(),
            },
        )
        .unwrap();
        for bytes in [false, true] {
            let cause = owner
                .bind(py)
                .call_method1("_dump_graph_nt", (py.None(), bytes))
                .unwrap_err();
            assert_eq!(cause.get_type(py).name().unwrap(), "Exception");
            assert_eq!(
                cause.value(py).str().unwrap().to_str().unwrap(),
                "\"urn:bad space\" does not look like a valid URI, I cannot serialize this as N3/Turtle. Perhaps you wanted to urlencode it?"
            );
        }
    });
    assert!(matches!(
        session.execute(Operation::Len).unwrap(),
        Reply::Count(1)
    ));
    session
        .execute(Operation::InsertTerms(Box::new(QuadInput::new(
            iri("urn:good"),
            iri("urn:p"),
            iri("urn:o"),
            None,
        ))))
        .unwrap();
    assert!(store.is_empty().unwrap());
    session.finish(true).unwrap();
    assert_eq!(store.len().unwrap(), 2);
}

#[test]
fn generalized_numeric_warning_order_does_not_abort_pending_session() {
    for warning_first in [false, true] {
        let store = Store::new().unwrap();
        let session =
            Arc::new(Session::start(Arc::new(Mutex::new(Control::new(store.clone())))).unwrap());
        let uri = |value: &str| ("uri".into(), value.into(), None, None);
        let weird = (
            "literal".into(),
            "bad".into(),
            Some("http://www.w3.org/2001/XMLSchema#double".into()),
            None,
        );
        let invalid = uri("urn:bad space");
        let (subject, predicate) = if warning_first {
            (weird, invalid)
        } else {
            (invalid, weird)
        };
        session
            .execute(Operation::InsertTerms(Box::new(QuadInput::new(
                subject,
                predicate,
                uri("urn:o"),
                None,
            ))))
            .unwrap();
        Python::initialize();
        Python::attach(|py| {
            let owner = Py::new(
                py,
                NativeTransaction {
                    session: session.clone(),
                },
            )
            .unwrap();
            let globals = PyDict::new(py);
            globals.set_item("owner", owner).unwrap();
            globals.set_item("warning_first", warning_first).unwrap();
            py.run(c_str!(r#"
import warnings
expected_warning = "Serializing weird numerical rdflib.term.Literal('bad', datatype=rdflib.term.URIRef('http://www.w3.org/2001/XMLSchema#double'))"
for as_bytes in (False, True):
    for filter_value in ('always', 'error'):
        with warnings.catch_warnings(record=True) as observed:
            warnings.simplefilter(filter_value)
            try:
                owner._dump_graph_nt(None, as_bytes)
            except Exception as error:
                if warning_first and filter_value == 'error':
                    assert type(error) is UserWarning
                    assert str(error) == expected_warning
                else:
                    assert type(error) is Exception
                    assert str(error) == '"urn:bad space" does not look like a valid URI, I cannot serialize this as N3/Turtle. Perhaps you wanted to urlencode it?'
            else:
                raise AssertionError('Invalid URI must reject the document')
            assert [(item.category, str(item.message)) for item in observed] == (
                [(UserWarning, expected_warning)] if warning_first and filter_value == 'always' else []
            )
"#), Some(&globals), None).unwrap();
        });
        assert!(matches!(
            session.execute(Operation::Len).unwrap(),
            Reply::Count(1)
        ));
        session
            .execute(Operation::InsertTerms(Box::new(QuadInput::new(
                uri("urn:still"),
                uri("urn:p"),
                uri("urn:usable"),
                None,
            ))))
            .unwrap();
        assert!(store.is_empty().unwrap());
        session.finish(false).unwrap();
        assert!(store.is_empty().unwrap());
    }
}
