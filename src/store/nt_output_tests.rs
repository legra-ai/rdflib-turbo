//! Exact private str/bytes output contracts on committed and pending owners.
use std::sync::{Arc, Mutex};

use oxigraph::model::{GraphName, Literal, NamedNode, Quad};
use oxigraph::store::Store;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyString};

use super::operations::Operation;
use super::session::{Control, Session};
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
