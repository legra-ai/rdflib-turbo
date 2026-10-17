use super::*;
use crate::graph::{RDF, iri};

fn dataset(source: &str) -> Dataset {
    RdfParser::from_format(RdfFormat::Turtle)
        .for_slice(source.as_bytes())
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap()
        .into_iter()
        .collect()
}

#[test]
fn graph_helpers_preserve_context_and_exact_subject_identity() {
    let data = dataset("<urn:s> <urn:p> <urn:o> . _:b <urn:p> <urn:o> .");
    let predicate = NamedNode::new("urn:p").unwrap();
    let object: Term = NamedNode::new("urn:o").unwrap().into();
    let named: Term = NamedNode::new("urn:s").unwrap().into();
    let blank: Term = BlankNode::new("b").unwrap().into();
    let literal: Term = Literal::new_simple_literal("invalid subject").into();
    assert_eq!(graph::subject(&named).unwrap().to_string(), "<urn:s>");
    assert_eq!(graph::subject(&blank).unwrap().to_string(), "_:b");
    assert!(
        graph::subject(&literal)
            .unwrap_err()
            .to_string()
            .contains("Expected a shape")
    );
    assert_eq!(
        graph::objects(&data, &named, &predicate).unwrap(),
        vec![object.clone()]
    );
    assert!(
        graph::objects(&data, &literal, &predicate)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        graph::subjects(&data, &predicate, Some(&object))
            .unwrap()
            .len(),
        2
    );
    assert_eq!(graph::subjects(&data, &predicate, None).unwrap().len(), 2);
    let mut before = data.clone();
    before.insert(&Quad::new(
        NamedNode::new("urn:n").unwrap(),
        predicate.clone(),
        object.clone(),
        NamedNode::new("urn:g").unwrap(),
    ));
    let union = graph::with_union(&before);
    assert_eq!(union.len(), before.len() + 1);
    let mut after = union.clone();
    after.insert(&Quad::new(
        NamedNode::new("urn:new").unwrap(),
        predicate,
        object,
        GraphName::DefaultGraph,
    ));
    let mut committed = before.clone();
    graph::commit_additions(&mut committed, &union, &after);
    assert_eq!(committed.len(), before.len() + 1);
    assert!(!committed.contains(QuadRef::new(
        NamedNodeRef::new_unchecked("urn:n"),
        NamedNodeRef::new_unchecked("urn:p"),
        NamedNodeRef::new_unchecked("urn:o"),
        GraphNameRef::DefaultGraph
    )));
}

#[test]
fn rdf_lists_validate_every_link_and_reject_ambiguous_or_cyclic_structure() {
    let head: Term = NamedNode::new("urn:head").unwrap().into();
    assert!(
        path::rdf_list(&Dataset::new(), &iri(RDF, "nil").into())
            .unwrap()
            .is_empty()
    );
    let valid = dataset(
        "<urn:head> <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <urn:a>; <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> <http://www.w3.org/1999/02/22-rdf-syntax-ns#nil> .",
    );
    assert_eq!(
        path::rdf_list(&valid, &head).unwrap(),
        vec![Term::from(NamedNode::new("urn:a").unwrap())]
    );
    for source in [
        "<urn:head> <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <urn:a> .",
        "<urn:head> <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <urn:a>, <urn:b>; <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> <http://www.w3.org/1999/02/22-rdf-syntax-ns#nil> .",
        "<urn:head> <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <urn:a>; <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> <urn:a>, <urn:b> .",
    ] {
        assert!(
            path::rdf_list(&dataset(source), &head)
                .unwrap_err()
                .to_string()
                .contains("Malformed RDF list")
        );
    }
    let cycle = dataset(
        "<urn:head> <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <urn:a>; <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> <urn:head> .",
    );
    assert!(
        path::rdf_list(&cycle, &head)
            .unwrap_err()
            .to_string()
            .contains("Cyclic RDF list")
    );
    assert!(path::rdf_list(&valid, &Literal::new_simple_literal("head").into()).is_err());
}

#[test]
fn inference_noop_modes_and_invalid_modes_are_explicit() {
    let original = dataset("<urn:s> <urn:p> <urn:o> .");
    for mode in ["none", ""] {
        let mut data = original.clone();
        inference::expand(&mut data, mode).unwrap();
        assert_eq!(data, original);
    }
    assert!(matches!(
        inference::expand(&mut original.clone(), "invalid"),
        Err(Error::Unsupported(_))
    ));
}

#[test]
fn poisoned_dataset_locks_fail_every_public_native_operation() {
    Python::initialize();
    let native = NativeDataset::new();
    assert!(
        std::panic::catch_unwind(|| {
            let _guard = native.inner.write().unwrap();
            panic!("simulate an interrupted native mutation");
        })
        .is_err()
    );
    Python::attach(|py| {
        let failures = [
            native.__len__().unwrap_err(),
            native.copy(py).err().unwrap(),
            native.load(py, "", "turtle", None, false).unwrap_err(),
            native.dump(py, "turtle").unwrap_err(),
            native.quads_json(py).unwrap_err(),
            native.query(py, "ASK {}", false).unwrap_err(),
            native.infer(py, "owlrl", None).unwrap_err(),
        ];
        for failure in failures {
            assert!(failure.is_instance_of::<PyRuntimeError>(py));
            assert!(failure.to_string().contains("Dataset lock poisoned"));
        }
    });
}

#[test]
fn native_errors_preserve_exception_categories_and_messages() {
    Python::initialize();
    Python::attach(|py| {
        let invalid = error(Error::Invalid("bad RDF".into()));
        assert!(invalid.is_instance_of::<PyValueError>(py));
        assert!(invalid.to_string().contains("bad RDF"));
        let unsupported = Error::Unsupported("network".into());
        assert_eq!(format!("{unsupported:?}"), "Unsupported(\"network\")");
        assert!(error(unsupported).is_instance_of::<PyNotImplementedError>(py));
        assert!(error(Error::Runtime("failed".into())).is_instance_of::<PyRuntimeError>(py));
    });
}

#[test]
fn native_input_validation_checks_formats_bases_modes_and_query_failures() {
    Python::initialize();
    Python::attach(|py| {
        let native = NativeDataset::new();
        for name in [
            "turtle", "ttl", "nt", "ntriples", "nquads", "nq", "trig", "xml", "rdfxml", "n3",
            "json-ld", "jsonld",
        ] {
            assert!(format(name).is_ok(), "{name}");
        }
        assert!(
            native
                .dump(py, "unknown")
                .unwrap_err()
                .is_instance_of::<PyNotImplementedError>(py)
        );
        assert!(
            native
                .load(py, "", "turtle", Some("not an IRI"), false)
                .unwrap_err()
                .is_instance_of::<PyValueError>(py)
        );
        assert!(
            native
                .infer(py, "none", None)
                .unwrap_err()
                .is_instance_of::<PyValueError>(py)
        );
        assert!(
            native
                .infer(py, "owlrl", Some("invalid IRI"))
                .unwrap_err()
                .is_instance_of::<PyValueError>(py)
        );
        for query in [
            "ASK { SERVICE <https://example.org/unavailable> { ?s ?p ?o } }",
            "SELECT * WHERE { SERVICE <https://example.org/unavailable> { ?s ?p ?o } }",
            "CONSTRUCT { ?s ?p ?o } WHERE { SERVICE <https://example.org/unavailable> { ?s ?p ?o } }",
        ] {
            assert!(
                native
                    .query(py, query, false)
                    .unwrap_err()
                    .is_instance_of::<PyRuntimeError>(py)
            );
        }
        let query_error = SparqlEvaluator::new()
            .parse_query("ASK { SERVICE <https://example.org/unavailable> { ?s ?p ?o } }")
            .unwrap()
            .on_queryable_dataset(&Dataset::new())
            .execute()
            .err()
            .unwrap();
        let wrapped = Error::from(query_error);
        assert!(std::error::Error::source(&wrapped).is_some());
        assert!(format!("{wrapped:?}").contains("Query"));
        assert!(error(wrapped).is_instance_of::<PyRuntimeError>(py));
    });
}

#[test]
fn property_chain_validation_and_named_graph_inference_preserve_atomicity() {
    Python::initialize();
    Python::attach(|py| {
        for (source, message) in [
            (
                "_:p <http://www.w3.org/2002/07/owl#propertyChainAxiom> (<urn:p>) .",
                "predicate must be an IRI",
            ),
            (
                "<urn:p> <http://www.w3.org/2002/07/owl#propertyChainAxiom> (\"literal\") .",
                "members must be IRIs",
            ),
            (
                "<urn:C> <http://www.w3.org/2002/07/owl#hasKey> (\"literal\") .",
                "Key properties must be IRIs",
            ),
        ] {
            let native = NativeDataset::new();
            native.load(py, source, "turtle", None, true).unwrap();
            let before = native.dump(py, "nquads").unwrap();
            let failure = native.infer(py, "owlrl", None).unwrap_err();
            assert!(failure.to_string().contains(message), "{failure}");
            assert_eq!(native.dump(py, "nquads").unwrap(), before);
        }
        let native = NativeDataset::new();
        native.load(py, "<urn:g> { <urn:C> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <urn:D> . <urn:x> a <urn:C> . }", "trig", None, true).unwrap();
        native.infer(py, "rdfs", None).unwrap();
        assert_eq!(
            native
                .query(py, "ASK { <urn:x> a <urn:D> }", false)
                .unwrap()
                .1,
            "true"
        );
        assert_eq!(
            native
                .query(py, "ASK { GRAPH <urn:g> { <urn:x> a <urn:D> } }", false)
                .unwrap()
                .1,
            "false"
        );
        native.infer(py, "owlrl", Some("urn:inferred")).unwrap();
    });
}

#[test]
fn chain_and_key_entailments_use_every_step_and_every_exact_key_value() {
    let mut data = dataset(
        r#"
        @prefix owl: <http://www.w3.org/2002/07/owl#>.
        <urn:ancestor> owl:propertyChainAxiom (<urn:parent> <urn:parent>).
        <urn:inactive> owl:propertyChainAxiom ().
        <urn:a> <urn:parent> <urn:b>. <urn:b> <urn:parent> <urn:c>.
        <urn:Inactive> owl:hasKey ().
        <urn:Person> owl:hasKey (<urn:first> <urn:second>).
        <urn:x> a <urn:Person>; <urn:first> "a"; <urn:second> "shared".
        <urn:y> a <urn:Person>; <urn:first> "a"; <urn:second> "shared".
        <urn:z> a <urn:Person>; <urn:first> "a"; <urn:second> "different".
        <urn:missing> a <urn:Person>; <urn:first> "no match".
    "#,
    );
    inference::expand(&mut data, "owlrl").unwrap();
    let ask = |query| {
        let QueryResults::Boolean(value) = SparqlEvaluator::new()
            .parse_query(query)
            .unwrap()
            .on_queryable_dataset(&data)
            .execute()
            .unwrap()
        else {
            panic!("ASK result expected")
        };
        value
    };
    assert!(ask("ASK { <urn:a> <urn:ancestor> <urn:c> }"));
    assert!(ask(
        "ASK { <urn:x> <http://www.w3.org/2002/07/owl#sameAs> <urn:y> }"
    ));
    assert!(!ask(
        "ASK { <urn:x> <http://www.w3.org/2002/07/owl#sameAs> <urn:z> }"
    ));
}

#[test]
fn failed_inference_preserves_named_contexts_and_inference_destinations() {
    Python::initialize();
    Python::attach(|py| {
        for destination in [None, Some("urn:destination")] {
            let native = NativeDataset::new();
            native
                .load(
                    py,
                    "<urn:g> { <urn:C> <http://www.w3.org/2002/07/owl#hasKey> _:broken . }",
                    "trig",
                    None,
                    true,
                )
                .unwrap();
            let before = native.dump(py, "nquads").unwrap();
            assert!(native.infer(py, "owlrl", destination).is_err());
            assert_eq!(native.dump(py, "nquads").unwrap(), before);
        }
        let native = NativeDataset::new();
        native
            .load(py, "<urn:s> <urn:p> \"français\"@fr.", "turtle", None, true)
            .unwrap();
        for format in ["turtle", "nt", "nquads", "trig", "xml", "n3", "json-ld"] {
            let serialized = native.dump(py, format).unwrap();
            let restored = NativeDataset::new();
            restored.load(py, &serialized, format, None, true).unwrap();
            assert_eq!(restored.__len__().unwrap(), 1, "{format}");
        }
    });
}
