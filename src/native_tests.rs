use oxigraph::sparql::QueryResults;

use super::*;

pub(crate) fn host_regex_policy(py: Python<'_>) -> Py<shacl_bindings::NativeRegexPolicy> {
    let module = pyo3::types::PyModule::new(py, "native_host_policy_fixture").unwrap();
    shacl_bindings::register(&module, crate::regex_policy::build(py).unwrap()).unwrap();
    module.getattr("_REGEX_POLICY").unwrap().extract().unwrap()
}

#[test]
fn failure_after_an_inference_round_rolls_back_all_axioms_and_derived_facts() {
    Python::initialize();
    Python::attach(|py| {
        for format in ["turtle", "trig"] {
            for destination in [None, Some("urn:destination")] {
                let statements = "<urn:keyAlias> <http://www.w3.org/2000/01/rdf-schema#subPropertyOf> <http://www.w3.org/2002/07/owl#hasKey> . <urn:C> <urn:keyAlias> <urn:broken> . <urn:broken> <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <urn:key>; <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> <urn:broken> .";
                let source = if format == "trig" {
                    format!("<urn:original> {{ {statements} }}")
                } else {
                    statements.to_owned()
                };
                let native = NativeDataset::new();
                native.load(py, &source, format, None, true).unwrap();
                let before = native.dump(py, "nquads").unwrap();
                assert!(
                    native
                        .infer(py, "owlrl", destination, false, false, None)
                        .unwrap_err()
                        .to_string()
                        .contains("List contains a recursive rdf:rest reference")
                );
                assert_eq!(native.dump(py, "nquads").unwrap(), before);
                // The failed round must not leave the newly derived hasKey or
                // the OWL axioms behind. A corrected input can then succeed.
                native.inner.write().unwrap().data.remove(QuadRef::new(
                    NamedNodeRef::new_unchecked("urn:broken"),
                    NamedNodeRef::new_unchecked("http://www.w3.org/1999/02/22-rdf-syntax-ns#rest"),
                    NamedNodeRef::new_unchecked("urn:broken"),
                    if format == "trig" {
                        GraphNameRef::NamedNode(NamedNodeRef::new_unchecked("urn:original"))
                    } else {
                        GraphNameRef::DefaultGraph
                    },
                ));
                native.load(py, "<urn:broken> <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <urn:key>; <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> <http://www.w3.org/1999/02/22-rdf-syntax-ns#nil> .", "turtle", None, true).unwrap();
                native
                    .infer(py, "owlrl", destination, false, false, None)
                    .unwrap();
            }
        }
    });
}

fn dataset(source: &str) -> Dataset {
    RdfParser::from_format(RdfFormat::Turtle)
        .for_slice(source.as_bytes())
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap()
        .into_iter()
        .collect()
}

#[test]
fn frozen_union_preserves_source_contexts_without_mutation() {
    let data = dataset("<urn:s> <urn:p> <urn:o> . _:b <urn:p> <urn:o> .");
    let predicate = NamedNode::new("urn:p").unwrap();
    let object: Term = NamedNode::new("urn:o").unwrap().into();
    let mut before = data.clone();
    before.insert(&Quad::new(
        NamedNode::new("urn:n").unwrap(),
        predicate.clone(),
        object.clone(),
        NamedNode::new("urn:g").unwrap(),
    ));
    let union = graph::with_union(&before.clone().into());
    assert_eq!(union.len(), before.len() + 1);
    assert!(!before.contains(QuadRef::new(
        NamedNodeRef::new_unchecked("urn:n"),
        NamedNodeRef::new_unchecked("urn:p"),
        NamedNodeRef::new_unchecked("urn:o"),
        GraphNameRef::DefaultGraph
    )));
}

#[test]
fn inference_noop_modes_and_invalid_modes_are_explicit() {
    let original: oxrdf::GeneralizedDataset = dataset("<urn:s> <urn:p> <urn:o> .").into();
    for mode in ["none", ""] {
        for options in [
            Default::default(),
            inference::InferenceOptions {
                axiomatic_triples: true,
                datatype_axioms: true,
            },
        ] {
            let mut data = original.clone();
            inference::expand(&mut data, mode, options.into()).unwrap();
            assert_eq!(data, original);
        }
    }
    assert!(matches!(
        inference::expand(&mut original.clone(), "invalid", Default::default()),
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
            native
                .infer(py, "owlrl", None, false, false, None)
                .unwrap_err(),
            native.namespaces().unwrap_err(),
            native.bind_namespaces(Vec::new()).unwrap_err(),
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
                .infer(py, "none", None, false, false, None)
                .unwrap_err()
                .is_instance_of::<PyValueError>(py)
        );
        assert!(
            native
                .infer(py, "owlrl", Some("invalid IRI"), false, false, None)
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
fn generalized_chain_key_schema_and_named_graph_inference_remain_supported() {
    Python::initialize();
    Python::attach(|py| {
        for source in [
            "_:p <http://www.w3.org/2002/07/owl#propertyChainAxiom> (<urn:p>) .",
            "<urn:p> <http://www.w3.org/2002/07/owl#propertyChainAxiom> (\"literal\") .",
            "<urn:C> <http://www.w3.org/2002/07/owl#hasKey> (\"literal\") .",
        ] {
            let native = NativeDataset::new();
            native.load(py, source, "turtle", None, true).unwrap();
            native.infer(py, "owlrl", None, false, false, None).unwrap();
        }
        let native = NativeDataset::new();
        native.load(py, "<urn:g> { <urn:C> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <urn:D> . <urn:x> a <urn:C> . }", "trig", None, true).unwrap();
        native.infer(py, "rdfs", None, false, false, None).unwrap();
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
        native
            .infer(py, "owlrl", Some("urn:inferred"), false, false, None)
            .unwrap();
    });
}

#[test]
fn chain_and_key_entailments_use_every_step_and_every_exact_key_value() {
    let mut data: oxrdf::GeneralizedDataset = dataset(
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
    )
    .into();
    inference::expand(&mut data, "owlrl", Default::default()).unwrap();
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
                    "<urn:g> { <urn:C> <http://www.w3.org/2002/07/owl#hasKey> _:broken . _:broken <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <urn:key>; <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> _:broken . }",
                    "trig",
                    None,
                    true,
                )
                .unwrap();
            let before = native.dump(py, "nquads").unwrap();
            assert!(
                native
                    .infer(py, "owlrl", destination, false, false, None)
                    .is_err()
            );
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
