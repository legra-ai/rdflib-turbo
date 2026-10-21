//! Checked owner-local revisions protect warning callback commit gaps.
use super::*;

const RULE: &str = "@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:S> sh:targetNode <urn:a>; sh:rule [a sh:TripleRule; sh:subject sh:this; sh:predicate <urn:p>; sh:object <urn:b>].";

#[test]
fn every_mutator_rejects_exhausted_revision_before_changes() {
    Python::initialize();
    Python::attach(|py| {
        let native = NativeDataset::new();
        native
            .load(py, "<urn:a> <urn:p> <urn:b>.", "turtle", None, true)
            .unwrap();
        native.inner.write().unwrap().revision = u64::MAX;
        let before = native.dump(py, "nquads").unwrap();
        let namespaces = native.namespaces().unwrap();
        let shapes = shacl::SharedShapes::new(py, RULE, true, vec![], true).unwrap();
        let failures = [
            native
                .load(py, "@prefix fresh:<urn:fresh:>.", "turtle", None, true)
                .unwrap_err(),
            native
                .bind_namespaces(vec![("fresh".into(), "urn:fresh:".into())])
                .unwrap_err(),
            native.infer(py, "rdfs", None, false, false).unwrap_err(),
            native
                .infer_delta(py, "rdfs", None, false, false)
                .err()
                .unwrap(),
            shapes.validate(py, &native, "{}", true).unwrap_err(),
            shapes.expand(py, &native, "{}", true).err().unwrap(),
        ];
        for failure in failures {
            assert!(failure.is_instance_of::<PyRuntimeError>(py));
            assert!(failure.to_string().contains("revision exhausted"));
        }
        assert_eq!(native.dump(py, "nquads").unwrap(), before);
        assert_eq!(native.namespaces().unwrap(), namespaces);
        assert_eq!(native.inner.read().unwrap().revision, u64::MAX);
    });
}

#[test]
fn successful_noop_writes_advance_once_and_copies_are_independent() {
    Python::initialize();
    Python::attach(|py| {
        let native = NativeDataset::new();
        native
            .load(py, "<urn:a> <urn:p> <urn:b>.", "turtle", None, true)
            .unwrap();
        assert_eq!(native.inner.read().unwrap().revision, 1);
        native.bind_namespaces(vec![]).unwrap();
        assert_eq!(native.inner.read().unwrap().revision, 2);
        native.infer(py, "rdfs", None, false, false).unwrap();
        assert_eq!(native.inner.read().unwrap().revision, 3);
        native.infer_delta(py, "rdfs", None, false, false).unwrap();
        assert_eq!(native.inner.read().unwrap().revision, 4);
        let copied = native.copy(py).unwrap();
        assert_eq!(copied.inner.read().unwrap().revision, 0);
        copied.bind_namespaces(vec![]).unwrap();
        assert_eq!(native.inner.read().unwrap().revision, 4);
        let shapes = shacl::SharedShapes::new(py, RULE, true, vec![], true).unwrap();
        shapes.validate(py, &native, "{}", true).unwrap();
        assert_eq!(native.inner.read().unwrap().revision, 5);
        shapes.validate(py, &native, "{}", false).unwrap();
        assert_eq!(native.inner.read().unwrap().revision, 5);
    });
}
