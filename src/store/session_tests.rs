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
            .execute(Operation::Infer("rdfs".into(), Some("urn:inferred".into())))
            .unwrap();
        let Reply::Query(_, result) = session
            .execute(Operation::Query(
                "ASK { GRAPH <urn:inferred> { <urn:a> a <urn:D> } }".into(),
                false,
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
                .execute(Operation::Infer(mode.into(), destination.map(String::from)))
                .is_err()
        );
        assert!(store.is_empty().unwrap());
        assert!(session.finish(true).is_err());
    }
}
