//! A transaction stays on its native owner thread; no borrowed handle escapes.
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};

use oxigraph::store::{Store, Transaction};

use crate::{Error, Result};

use super::operations::{Operation, Reply};

pub(super) struct Control {
    pub store: Option<Store>,
    active_writer: bool,
}

impl Control {
    pub fn new(store: Store) -> Self {
        Self {
            store: Some(store),
            active_writer: false,
        }
    }

    pub fn read(&self) -> Result<Store> {
        self.store.clone().ok_or_else(closed)
    }

    pub fn close(&mut self) -> Result<()> {
        if self.active_writer {
            return Err(Error::Runtime("Store has an active transaction".into()));
        }
        self.store.take();
        Ok(())
    }
}

fn closed() -> Error {
    Error::Runtime("Store or transaction is closed".into())
}

fn failed() -> Error {
    Error::Runtime("Native transaction worker failed".into())
}

pub(super) fn lock<T>(value: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>> {
    value
        .lock()
        .map_err(|_| Error::Runtime("Store lock poisoned".into()))
}

struct WriterPermit(Arc<Mutex<Control>>);

impl Drop for WriterPermit {
    fn drop(&mut self) {
        // Poison remains visible to callers; never recover a possibly invalid store.
        if let Ok(mut control) = self.0.lock() {
            control.active_writer = false;
        }
    }
}

enum Command {
    Execute(Operation, mpsc::SyncSender<Result<Reply>>),
    Finish(bool, mpsc::SyncSender<Result<Reply>>),
}

struct Connection {
    sender: Option<mpsc::SyncSender<Command>>,
    worker: Option<JoinHandle<()>>,
}

impl Connection {
    fn join(&mut self) -> Result<()> {
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            worker.join().map_err(|_| failed())?;
        }
        Ok(())
    }

    fn call(
        &mut self,
        command: impl FnOnce(mpsc::SyncSender<Result<Reply>>) -> Command,
    ) -> Result<Reply> {
        let sender = self.sender.as_ref().ok_or_else(closed)?;
        // bounded: one synchronous request and one response per caller, no queued jobs.
        let (reply, receiver) = mpsc::sync_channel(0);
        let result = sender
            .send(command(reply))
            .map_err(|_| failed())
            .and_then(|()| receiver.recv().map_err(|_| failed())?);
        if result.is_err() {
            self.join()?;
        }
        result
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        // Disconnect aborts on the worker, then joins before releasing the handle.
        let _ = self.join();
    }
}

pub(super) struct Session(Mutex<Connection>);

fn reserve_writer(control: &Arc<Mutex<Control>>) -> Result<(Store, WriterPermit)> {
    let store = {
        let mut state = lock(control)?;
        let store = state.read()?;
        if state.active_writer {
            return Err(Error::Runtime("Store has an active transaction".into()));
        }
        state.active_writer = true;
        store
    };
    Ok((store, WriterPermit(control.clone())))
}

impl Session {
    pub fn execute_inline(control: Arc<Mutex<Control>>, operation: Operation) -> Result<Reply> {
        // An implicit transaction never outlives this native call. Borrow it on
        // the caller thread; only explicit multi-call sessions require an owner.
        let (store, _permit) = reserve_writer(&control)?;
        let mut transaction = store
            .start_transaction()
            .map_err(|e| Error::Runtime(e.to_string()))?;
        let reply = operation.execute(&mut transaction)?;
        transaction
            .commit()
            .map_err(|e| Error::Runtime(e.to_string()))?;
        Ok(reply)
    }

    pub fn start(control: Arc<Mutex<Control>>) -> Result<Self> {
        let (store, permit) = reserve_writer(&control)?;
        // bounded: rendezvous channels retain at most the current command/response.
        let (sender, receiver) = mpsc::sync_channel(0);
        let (ready, started) = mpsc::sync_channel(0);
        let worker = thread::Builder::new()
            .name("rdf-transaction".into())
            .spawn(move || {
                let _permit = permit;
                let transaction = store
                    .start_transaction()
                    .map_err(|e| Error::Runtime(e.to_string()));
                match transaction {
                    Ok(transaction) => {
                        if ready.send(Ok(())).is_ok() {
                            run(transaction, receiver);
                        }
                    }
                    Err(error) => {
                        let _ = ready.send(Err(error));
                    }
                }
            })
            .map_err(|e| Error::Runtime(e.to_string()))?;
        let session = Self(Mutex::new(Connection {
            sender: Some(sender),
            worker: Some(worker),
        }));
        started.recv().map_err(|_| failed())??;
        Ok(session)
    }

    pub fn execute(&self, operation: Operation) -> Result<Reply> {
        lock(&self.0)?.call(|reply| Command::Execute(operation, reply))
    }

    pub fn finish(&self, commit: bool) -> Result<()> {
        let mut connection = lock(&self.0)?;
        if connection.sender.is_none() && !commit {
            return Ok(());
        }
        let result = connection.call(|reply| Command::Finish(commit, reply));
        connection.join()?;
        result.map(|_| ())
    }
}

fn run(mut transaction: Transaction<'_>, receiver: mpsc::Receiver<Command>) {
    while let Ok(command) = receiver.recv() {
        match command {
            Command::Execute(operation, reply) => {
                let result = operation.execute(&mut transaction);
                let abort = result.is_err();
                if reply.send(result).is_err() || abort {
                    return;
                }
            }
            Command::Finish(commit, reply) => {
                let result = if commit {
                    transaction
                        .commit()
                        .map_err(|e| Error::Runtime(e.to_string()))
                } else {
                    drop(transaction);
                    Ok(())
                };
                let _ = reply.send(result.map(|()| Reply::Done));
                return;
            }
        }
    }
}

#[cfg(test)]
#[path = "session_failure_tests.rs"]
mod failure_tests;
