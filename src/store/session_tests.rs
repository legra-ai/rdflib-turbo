use std::sync::{Arc, Mutex};

use oxigraph::store::Store;

use super::operations::{Operation, Reply};
use super::session::{Control, Session};

fn control() -> Arc<Mutex<Control>> {
    Arc::new(Mutex::new(Control::new(Store::new().unwrap())))
}

fn insert() -> Operation {
    Operation::Update("INSERT DATA { <urn:s> <urn:p> <urn:o> }".into())
}

#[test]
fn dropping_a_native_handle_aborts_and_releases_the_writer() {
    let control = control();
    let store = control.lock().unwrap().read().unwrap();
    let session = Session::start(control.clone()).unwrap();
    session.execute(insert()).unwrap();
    assert!(matches!(
        session.execute(Operation::Len).unwrap(),
        Reply::Count(1)
    ));
    assert_eq!(store.len().unwrap(), 0);
    drop(session);
    assert_eq!(store.len().unwrap(), 0);
    let session = Session::start(control).unwrap();
    session.execute(insert()).unwrap();
    session.finish(true).unwrap();
    assert_eq!(store.len().unwrap(), 1);
    assert!(session.execute(Operation::Len).is_err());
    assert!(session.finish(true).is_err());
    session.finish(false).unwrap();
}

#[test]
fn native_operation_error_aborts_before_the_next_writer_can_start() {
    let control = control();
    let store = control.lock().unwrap().read().unwrap();
    let session = Session::start(control.clone()).unwrap();
    session.execute(insert()).unwrap();
    assert!(
        session
            .execute(Operation::Update("INVALID UPDATE".into()))
            .is_err()
    );
    assert_eq!(store.len().unwrap(), 0);
    assert!(session.finish(true).is_err());
    session.finish(false).unwrap();
    let next = Session::start(control).unwrap();
    next.execute(insert()).unwrap();
    next.finish(true).unwrap();
    assert_eq!(store.len().unwrap(), 1);
}

#[test]
fn writer_reservations_and_closed_handles_fail_immediately() {
    let control = control();
    let session = Session::start(control.clone()).unwrap();
    assert!(Session::start(control.clone()).is_err());
    assert!(control.lock().unwrap().close().is_err());
    session.finish(false).unwrap();
    control.lock().unwrap().close().unwrap();
    assert!(Session::start(control.clone()).is_err());
    assert!(control.lock().unwrap().read().is_err());
}

#[test]
fn transaction_commands_can_move_between_caller_threads() {
    let control = control();
    let store = control.lock().unwrap().read().unwrap();
    let session = Arc::new(Session::start(control).unwrap());
    let client = session.clone();
    std::thread::spawn(move || {
        client.execute(insert()).unwrap();
    })
    .join()
    .unwrap();
    assert!(matches!(
        session.execute(Operation::Len).unwrap(),
        Reply::Count(1)
    ));
    session.finish(true).unwrap();
    assert_eq!(store.len().unwrap(), 1);
}

#[test]
fn inference_uses_pending_writes_and_the_same_commit_boundary() {
    let control = control();
    let store = control.lock().unwrap().read().unwrap();
    for commit in [false, true] {
        let session = Session::start(control.clone()).unwrap();
        session.execute(Operation::Update("INSERT DATA { GRAPH <urn:g> { <urn:a> a <urn:C> . <urn:C> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <urn:D> } }".into())).unwrap();
        session
            .execute(Operation::Infer(
                "rdfs".into(),
                Some("urn:inferred".into()),
                Default::default(),
            ))
            .unwrap();
        let Reply::Query(_, result) = session
            .execute(Operation::Query(
                "ASK { GRAPH <urn:inferred> { <urn:a> a <urn:D> } }".into(),
                false,
                None,
            ))
            .unwrap()
        else {
            panic!("query reply")
        };
        assert!(result.contains("true"));
        assert!(store.is_empty().unwrap());
        session.finish(commit).unwrap();
        assert_eq!(store.is_empty().unwrap(), !commit);
    }
}

#[test]
fn invalid_inference_aborts_all_prior_changes_and_releases_the_writer() {
    let control = control();
    let store = control.lock().unwrap().read().unwrap();
    for (mode, destination) in [("invalid", None), ("rdfs", Some("not an iri"))] {
        let session = Session::start(control.clone()).unwrap();
        session.execute(insert()).unwrap();
        assert!(
            session
                .execute(Operation::Infer(
                    mode.into(),
                    destination.map(String::from),
                    Default::default()
                ))
                .is_err()
        );
        assert!(store.is_empty().unwrap());
        assert!(session.finish(true).is_err());
    }
}

#[test]
fn implicit_mutations_commit_abort_and_release_the_writer_without_an_owner_thread() {
    let control = control();
    let store = control.lock().unwrap().read().unwrap();
    Session::execute_inline(control.clone(), insert()).unwrap();
    assert_eq!(store.len().unwrap(), 1);
    assert!(
        Session::execute_inline(
            control.clone(),
            Operation::Update("INSERT DATA { <urn:s2> <urn:p> <urn:o> }; INVALID".into())
        )
        .is_err()
    );
    assert_eq!(store.len().unwrap(), 1);
    let session = Session::start(control.clone()).unwrap();
    assert!(Session::execute_inline(control.clone(), insert()).is_err());
    session.finish(false).unwrap();
    Session::execute_inline(control.clone(), insert()).unwrap();
    control.lock().unwrap().close().unwrap();
    assert!(Session::execute_inline(control, insert()).is_err());
}

#[test]
fn poisoned_store_control_is_a_hard_error_at_the_native_boundary() {
    pyo3::Python::initialize();
    let store = pyo3::Python::attach(|py| super::NativeStore::new(py, None).unwrap());
    let control = store.control.clone();
    assert!(
        std::thread::spawn(move || {
            let _guard = control.lock().unwrap();
            panic!("simulate a panic while holding the native store control");
        })
        .join()
        .is_err()
    );
    pyo3::Python::attach(|py| {
        assert!(
            store
                .ensure_open()
                .unwrap_err()
                .to_string()
                .contains("poisoned")
        );
        assert!(
            store
                .__len__(py)
                .unwrap_err()
                .to_string()
                .contains("poisoned")
        );
        assert!(
            store
                .close(py)
                .unwrap_err()
                .to_string()
                .contains("poisoned")
        );
        let failure = store.transaction(py).err().unwrap();
        assert!(failure.to_string().contains("poisoned"));
    });
    let failure = Session::execute_inline(store.control.clone(), insert())
        .err()
        .unwrap();
    assert!(failure.to_string().contains("poisoned"));
}

#[cfg(not(feature = "persistence"))]
#[test]
fn memory_only_native_build_rejects_persistent_paths() {
    pyo3::Python::initialize();
    pyo3::Python::attach(|py| {
        let failure = super::NativeStore::new(py, Some("must-not-open-a-database".into()))
            .err()
            .unwrap();
        assert!(failure.is_instance_of::<pyo3::exceptions::PyNotImplementedError>(py));
        assert!(
            failure
                .to_string()
                .contains("no RocksDB persistence support")
        );
    });
}
