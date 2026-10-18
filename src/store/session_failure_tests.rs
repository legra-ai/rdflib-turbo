use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use oxigraph::store::Store;

use super::{Command, Connection, Control, Session, lock, reserve_writer};
use crate::store::operations::Operation;

fn insert() -> Operation {
    Operation::Update("INSERT DATA { <urn:s> <urn:p> <urn:o> }".into())
}

fn owner_failure_aborts_and_releases_writer(panic_after_write: bool) {
    let store = Store::new().unwrap();
    let control = Arc::new(Mutex::new(Control::new(store.clone())));
    let (worker_store, permit) = reserve_writer(&control).unwrap();
    // bounded: a rendezvous command and one independent test-stage acknowledgement.
    let (sender, receiver) = mpsc::sync_channel(0);
    let (staged, observed) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        let _permit = permit;
        let mut transaction = worker_store.start_transaction().unwrap();
        let Command::Execute(operation, reply) = receiver.recv().unwrap() else {
            panic!("Expected a real native operation");
        };
        operation.execute(&mut transaction).unwrap();
        staged.send(transaction.len().unwrap()).unwrap();
        drop(reply);
        if panic_after_write {
            panic!("Native owner failed after staging its writes");
        }
        // Returning without a reply models an unexpected owner disconnection.
        // The transaction and real writer permit must still unwind normally.
    });
    let session = Session(Mutex::new(Connection {
        sender: Some(sender),
        worker: Some(worker),
    }));
    let error = session.execute(insert()).err().unwrap();
    assert!(
        error
            .to_string()
            .contains("Native transaction worker failed")
    );
    assert_eq!(observed.recv().unwrap(), 1);
    let connection = lock(&session.0).unwrap();
    assert!(connection.sender.is_none());
    assert!(connection.worker.is_none());
    drop(connection);
    assert_eq!(store.len().unwrap(), 0);
    assert!(session.finish(true).is_err());
    session.finish(false).unwrap();
    let next = Session::start(control).unwrap();
    next.execute(insert()).unwrap();
    next.finish(true).unwrap();
    assert_eq!(store.len().unwrap(), 1);
}

#[test]
fn owner_disconnection_before_reply_joins_aborts_and_releases_writer() {
    owner_failure_aborts_and_releases_writer(false);
}

#[test]
fn owner_panic_before_reply_joins_aborts_and_releases_writer() {
    owner_failure_aborts_and_releases_writer(true);
}
