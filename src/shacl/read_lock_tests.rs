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

#[test]
fn dataset_preinference_uses_read_lease_and_keeps_derived_output_private() {
    Python::initialize();
    Python::attach(|py| {
        let shapes = SharedShapes::new(py,
            "@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:S> sh:targetSubjectsOf <urn:parent>; sh:class <urn:Required>.",
            false, vec![], true).unwrap();
        let owner = pyo3::Py::new(py, NativeDataset::new()).unwrap();
        let dataset = owner.bind(py).get();
        dataset.load(py, "<urn:s> <urn:child> <urn:o>. <urn:child> <http://www.w3.org/2000/01/rdf-schema#subPropertyOf> <urn:parent>.", "turtle", None, false).unwrap();
        let before = dataset.dump(py, "nquads").unwrap();
        for mode in ["rdfs", "owlrl", "both"] {
            let settings = format!("{{\"inference\":\"{mode}\"}}");
            let reader = dataset.inner.read().unwrap();
            let prepared = shapes
                .prepare_dataset(py, owner.clone_ref(py), &settings, true)
                .unwrap();
            assert!(!prepared.output().unwrap().0);
            drop(prepared);
            assert_eq!(reader.data.len(), 2);
            drop(reader);
            assert_eq!(dataset.dump(py, "nquads").unwrap(), before);
            assert!(!shapes.validate(py, dataset, &settings, false).unwrap().0);
            assert_eq!(dataset.dump(py, "nquads").unwrap(), before);
        }
        let prepared = shapes
            .prepare_dataset(py, owner.clone_ref(py), "{\"inference\":\"rdfs\"}", true)
            .unwrap();
        prepared.output().unwrap();
        prepared.accept(py).unwrap();
        assert!(dataset.__len__().unwrap() > 2);
    });
}

#[test]
fn diagnostic_namespace_is_private_for_report_and_published_only_at_acceptance() {
    Python::initialize();
    Python::attach(|py| {
        let shapes = SharedShapes::new(py, "@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:S> sh:targetClass <http://www.daml.org/2002/03/agents/agent-ont#ErrorMessage>; sh:class <urn:Required>.", false, vec![], true).unwrap();
        let owner = pyo3::Py::new(py, NativeDataset::new()).unwrap();
        let dataset = owner.bind(py).get();
        dataset
            .load(
                py,
                "<urn:s> <urn:p> \"bad\"^^<http://www.w3.org/2001/XMLSchema#integer>.",
                "turtle",
                None,
                false,
            )
            .unwrap();
        let before = dataset.namespaces().unwrap();
        let prepared = shapes
            .prepare_dataset(py, owner.clone_ref(py), "{\"inference\":\"owlrl\"}", true)
            .unwrap();
        let output = prepared.output().unwrap();
        assert!(!output.0);
        assert!(output.2.contains("err:error"), "{}", output.2);
        assert_eq!(dataset.namespaces().unwrap(), before);
        prepared.accept(py).unwrap();
        assert!(dataset.namespaces().unwrap().contains(&(
            "err".into(),
            "http://www.daml.org/2002/03/agents/agent-ont#".into()
        )));
    });
}
