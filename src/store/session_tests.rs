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
