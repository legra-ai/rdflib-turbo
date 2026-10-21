//! Validation must share a read lease with other dataset readers.
use std::sync::{Arc, mpsc};
use std::time::Duration;

use super::SharedShapes;
use crate::NativeDataset;
use pyo3::Python;

#[test]
fn readonly_validation_can_run_while_another_reader_holds_the_dataset() {
    Python::initialize();
    for inplace in [false, true] {
        let shapes = Python::attach(|py| {
            SharedShapes::new(
            py,
            "@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:S> sh:targetNode <urn:s>; sh:class <urn:C>.",
            false,
            Vec::new(),
            true,
        )
        .unwrap()
        });
        let dataset = Arc::new(NativeDataset::new());
        let reader = dataset.inner.read().unwrap();
        let source = Arc::clone(&dataset);
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || {
            let result = Python::attach(|py| shapes.validate(py, &source, "{}", inplace));
            sender.send(result).unwrap();
        });
        let result = receiver.recv_timeout(Duration::from_secs(5));
        drop(reader);
        worker.join().unwrap();
        assert!(
            !result
                .expect("read-only validation requested an exclusive lock")
                .unwrap()
                .0
        );
    }
}

#[test]
fn readonly_report_acceptance_shares_existing_read_leases() {
    Python::initialize();
    let dataset = Arc::new(NativeDataset::new());
    let reader = dataset.inner.read().unwrap();
    let source = Arc::clone(&dataset);
    let (sender, receiver) = mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        let result = Python::attach(|py| super::commit::Pending::readonly(0).apply(py, &source));
        sender.send(result).unwrap();
    });
    let result = receiver.recv_timeout(Duration::from_secs(1));
    drop(reader);
    worker.join().unwrap();
    result
        .expect("readonly report acceptance requested an exclusive lock")
        .unwrap();
}
