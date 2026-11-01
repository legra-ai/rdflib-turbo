use super::*;
use oxigraph::io::RdfFormat;

fn rules() -> CompiledShapes {
    CompiledShapes::compile(crate::IndexedShapesSource::parse_graph("@prefix sh: <http://www.w3.org/ns/shacl#>. <urn:S> sh:targetNode <urn:s>; sh:rule [a sh:TripleRule; sh:subject sh:this; sh:predicate <urn:derived>; sh:object true].", RdfFormat::Turtle, None).unwrap(), true).unwrap()
}

#[test]
fn numeric_role_reports_retain_the_exact_operator_bound_and_pair_predicate() {
    let mut data = parse(
        "@prefix xsd:<http://www.w3.org/2001/XMLSchema#>. <urn:s> <urn:p> \"1.0\"^^xsd:double; <urn:q> \"0.9\"^^xsd:decimal.",
        RdfFormat::Turtle,
        None,
    )
    .unwrap();
    for (operator, parameter, expected) in [
        (
            "minExclusive",
            "\"1.1\"^^xsd:decimal",
            "Value is not > Literal(\"1.1\", datatype=xsd:decimal)",
        ),
        (
            "minInclusive",
            "\"1.1\"^^xsd:decimal",
            "Value is not >= Literal(\"1.1\", datatype=xsd:decimal)",
        ),
        (
            "maxExclusive",
            "\"0.9\"^^xsd:decimal",
            "Value is not < Literal(\"0.9\", datatype=xsd:decimal)",
        ),
        (
            "maxInclusive",
            "\"0.9\"^^xsd:decimal",
            "Value is not <= Literal(\"0.9\", datatype=xsd:decimal)",
        ),
        (
            "lessThan",
            "<urn:q>",
            "Value of <urn:s>-><urn:q> <= Literal(\"1.0\", datatype=xsd:double)",
        ),
        (
            "lessThanOrEquals",
            "<urn:q>",
            "Value of <urn:s>-><urn:q> < Literal(\"1.0\", datatype=xsd:double)",
        ),
    ] {
        let shapes = crate::IndexedShapesSource::parse_graph(
            &format!("@prefix sh:<http://www.w3.org/ns/shacl#>. @prefix xsd:<http://www.w3.org/2001/XMLSchema#>. <urn:S> sh:targetNode <urn:s>; sh:path <urn:p>; sh:{operator} {parameter}."),
            RdfFormat::Turtle,
            None,
        )
        .unwrap();
        let report = CompiledShapes::compile(shapes, false)
            .unwrap()
            .validate(&mut data, Options::default())
            .unwrap();
        assert!(!report.conforms, "{operator}");
        let messages: Vec<_> = report
            .graph
            .iter()
            .filter(|q| q.predicate == iri(SH, "resultMessage").as_ref())
            .map(|q| q.object.to_string())
            .collect();
        assert_eq!(
            messages,
            vec![Literal::new_simple_literal(expected).to_string()],
            "{operator}"
        );
        assert!(
            report.text.contains(&format!("Message: {expected}\n")),
            "{operator}: {}",
            report.text
        );
    }
}

#[test]
fn rust_store_validation_and_rule_apis_commit_only_when_requested() {
    let program = rules();
    for inplace in [false, true] {
        for validate in [false, true] {
            let store = oxigraph::store::Store::new().unwrap();
            store
                .insert(QuadRef::new(
                    NamedNodeRef::new_unchecked("urn:s"),
                    NamedNodeRef::new_unchecked("urn:p"),
                    LiteralRef::new_simple_literal("plain"),
                    GraphNameRef::DefaultGraph,
                ))
                .unwrap();
            if validate {
                let report = program
                    .validate_store(&store, Options::default(), inplace)
                    .unwrap();
                assert!(report.conforms);
            } else {
                let expansion = program
                    .expand_store(&store, Options::default(), inplace)
                    .unwrap();
                assert_eq!(expansion.graph.len(), 1);
                assert!(expansion.recursion_warnings.is_empty());
            }
            assert_eq!(store.len().unwrap(), if inplace { 2 } else { 1 });
            let snapshot: Dataset = store.iter().map(|quad| store_quad(quad.unwrap())).collect();
            assert!(snapshot.iter().any(|q| matches!(q.object, TermRef::Literal(l) if l.value()=="plain" && l.has_explicit_datatype())));
        }
    }
}

#[test]
fn membership_traversal_handles_cycles_literals_and_missing_classes() {
    let data = parse("<urn:s> a <urn:A>. <urn:A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <urn:B>. <urn:B> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <urn:A>.", RdfFormat::Turtle, None).unwrap();
    let node: Term = iri("urn:", "s").into();
    assert!(is_instance(&data, &node, &iri("urn:", "B").into()).unwrap());
    assert!(!is_instance(&data, &node, &iri("urn:", "absent").into()).unwrap());
    assert!(!is_instance(&data, &Literal::from(1).into(), &iri("urn:", "A").into()).unwrap());
    assert!(
        rules()
            .shape_focus_nodes("<urn:missing>", &data, true)
            .unwrap_err()
            .to_string()
            .contains("Unknown shape")
    );
    let options = Options::default();
    assert_eq!(options.clone().max_validation_depth, 15);
    assert!(format!("{options:?}").contains("max_rule_iterations"));
}

#[test]
fn borrowed_rule_expansion_returns_only_additions_without_mutating_source() {
    let data = parse("<urn:s> <urn:p> <urn:o>.", RdfFormat::Turtle, None).unwrap();
    let expansion = rules().expand_staged(&data, Options::default()).unwrap();
    assert_eq!(data.len(), 1);
    assert_eq!(expansion.graph.len(), 1);
    assert!(expansion.graph.iter().all(|quad| !data.contains(quad)));
}

#[test]
fn selected_graph_validation_and_function_queries_cannot_leak_backend_union() {
    let shapes = crate::IndexedShapesSource::parse_graph("@prefix sh: <http://www.w3.org/ns/shacl#>. <urn:lookup> a sh:SPARQLFunction; sh:parameter [sh:path <https://example.org/x>]; sh:select 'SELECT ?out WHERE { $x <urn:value> ?out }'. <urn:S> sh:targetNode <urn:s>; sh:sparql [sh:select 'SELECT $this WHERE { FILTER(<urn:lookup>($this) != 2) }']; sh:property [sh:path <urn:value>; sh:hasValue 2; sh:maxCount 1].", RdfFormat::Turtle, None).unwrap();
    let program = CompiledShapes::compile(shapes, true).unwrap();
    for physical in [
        GraphName::DefaultGraph,
        iri("urn:", "selected").into(),
        BlankNode::new("selected").unwrap().into(),
    ] {
        let store = oxigraph::store::Store::new().unwrap();
        let mut transaction = store.start_transaction().unwrap();
        for (graph, value) in [(physical.clone(), 2), (iri("urn:", "other").into(), 3)] {
            transaction.insert(&Quad::new(
                iri("urn:", "s"),
                iri("urn:", "value"),
                Literal::from(value),
                graph,
            ));
        }
        let view = crate::ScopedGraph::new(&transaction, physical);
        assert!(
            program
                .validate_readonly(&view, Options::default())
                .unwrap()
                .conforms
        );
        assert!(
            !program
                .validate_readonly(&transaction, Options::default())
                .unwrap()
                .conforms
        );
    }
}

#[test]
fn borrowed_rule_expansion_stages_only_additions_and_observes_pending_data() {
    let shapes = crate::IndexedShapesSource::parse_graph("@prefix sh: <http://www.w3.org/ns/shacl#>. <urn:S> sh:targetNode <urn:s>; sh:rule [a sh:SPARQLRule; sh:construct 'CONSTRUCT { $this <urn:derived> ?value } WHERE { $this <urn:value> ?value }']; sh:property [sh:path <urn:derived>; sh:hasValue 2].", RdfFormat::Turtle, None).unwrap();
    let program = CompiledShapes::compile(shapes, true).unwrap();
    let store = oxigraph::store::Store::new().unwrap();
    let mut transaction = store.start_transaction().unwrap();
    transaction.insert(&Quad::new(
        iri("urn:", "s"),
        iri("urn:", "value"),
        Literal::from(2),
        iri("urn:", "source"),
    ));
    let expanded = program
        .expand_staged(&transaction, Options::default())
        .unwrap();
    assert_eq!(expanded.graph.len(), 1);
    assert!(expanded.graph.contains(&Quad::new(
        iri("urn:", "s"),
        iri("urn:", "derived"),
        Literal::from(2),
        GraphName::DefaultGraph
    )));
    assert_eq!(transaction.len().unwrap(), 1);
    let staged = program
        .validate_delta(&transaction, Options::default())
        .unwrap();
    assert!(staged.report.conforms);
    assert_eq!(staged.additions.len(), 1);
    assert_eq!(transaction.len().unwrap(), 1);
    assert!(store.is_empty().unwrap());
}

#[test]
fn store_expansion_returns_only_staged_additions_and_preserves_source_on_failure() {
    let program = rules();
    let store = oxigraph::store::Store::new().unwrap();
    let original = Quad::new(
        iri("urn:", "s"),
        iri("urn:", "original"),
        Literal::from(1),
        GraphName::DefaultGraph,
    );
    store.insert(&original).unwrap();
    let expansion = program
        .expand_store(&store, Options::default(), false)
        .unwrap();
    assert_eq!(expansion.graph.len(), 1);
    assert!(!expansion.graph.contains(&original));
    assert_eq!(store.len().unwrap(), 1);
    assert!(
        program
            .expand_store(
                &store,
                Options {
                    iterate_rules: true,
                    max_rule_iterations: 1,
                    ..Options::default()
                },
                true
            )
            .is_err()
    );
    assert_eq!(store.len().unwrap(), 1);
    assert!(store.contains(&original).unwrap());
}

#[test]
fn borrowed_store_strings_keep_the_explicit_rdf11_adapter_identity() {
    let shapes = crate::IndexedShapesSource::parse_graph("@prefix sh: <http://www.w3.org/ns/shacl#>. <urn:S> sh:targetNode <urn:s>; sh:property [sh:path <urn:value>; sh:hasValue 'plain'^^<http://www.w3.org/2001/XMLSchema#string>].", RdfFormat::Turtle, None).unwrap();
    let program = CompiledShapes::compile(shapes, true).unwrap();
    let store = oxigraph::store::Store::new().unwrap();
    store
        .insert(
            Quad::new(
                iri("urn:", "s"),
                iri("urn:", "value"),
                Literal::new_simple_literal("plain"),
                GraphName::DefaultGraph,
            )
            .as_ref(),
        )
        .unwrap();
    assert!(
        program
            .validate_store(&store, Options::default(), false)
            .unwrap()
            .conforms
    );
}

#[test]
fn transaction_graph_reads_preserve_the_physical_term_identity() {
    let store = oxigraph::store::Store::new().unwrap();
    let mut transaction = store.start_transaction().unwrap();
    transaction.insert(&Quad::new(
        iri("urn:", "s"),
        iri("urn:", "p"),
        Literal::new_simple_literal("plain"),
        GraphName::DefaultGraph,
    ));
    let physical = transaction
        .quads_for_pattern(None, None, None, None)
        .next()
        .unwrap()
        .unwrap();
    let observed = transaction
        .matching_quads(None, None, None, None)
        .next()
        .unwrap()
        .unwrap();
    assert_eq!(observed, GeneralizedQuad::from(physical));
}

#[test]
fn abort_finishes_one_component_for_all_focuses_before_stopping() {
    for (severity, allowed, expected_results) in [
        ("Violation", false, 2),
        ("Warning", false, 2),
        ("Warning", true, 4),
        ("Info", true, 4),
    ] {
        let shapes = crate::IndexedShapesSource::parse_graph(
            &format!(
                "@prefix sh:<http://www.w3.org/ns/shacl#>. \
                 <urn:S> sh:targetNode <urn:a>,<urn:b>;sh:path <urn:p>;\
                 sh:minCount 2;sh:maxCount 0;sh:severity sh:{severity}."
            ),
            RdfFormat::Turtle,
            None,
        )
        .unwrap();
        let program = CompiledShapes::compile(shapes, false).unwrap();
        let data = parse(
            "<urn:a> <urn:p> 'plain'. <urn:b> <urn:p> 'plain'.",
            RdfFormat::Turtle,
            None,
        )
        .unwrap();
        let options = Options {
            abort_on_first: true,
            allow_infos: allowed,
            allow_warnings: allowed,
            ..Options::default()
        };
        let store = oxigraph::store::Store::new().unwrap();
        for quad in &data {
            store.insert(quad).unwrap();
        }
        for report in [
            program.validate_readonly(&data, options.clone()).unwrap(),
            program.validate_store(&store, options, false).unwrap(),
        ] {
            let components: Vec<_> = report
                .graph
                .iter()
                .filter(|q| q.predicate == iri(SH, "sourceConstraintComponent").as_ref())
                .map(|q| q.object.into_owned())
                .collect();
            assert_eq!(components.len(), expected_results, "{severity}/{allowed}");
            assert_eq!(report.conforms, allowed);
            if !allowed {
                assert!(
                    components
                        .iter()
                        .all(|c| c == &iri(SH, "MinCountConstraintComponent").into())
                );
                let focuses: HashSet<_> = report
                    .graph
                    .iter()
                    .filter(|q| q.predicate == iri(SH, "focusNode").as_ref())
                    .map(|q| q.object.into_owned())
                    .collect();
                assert_eq!(focuses.len(), 2);
            }
        }
    }
}

#[test]
fn abort_keeps_all_declarations_within_the_failing_component() {
    let shapes = crate::IndexedShapesSource::parse_graph(
        "@prefix sh:<http://www.w3.org/ns/shacl#>. \
         <urn:S> sh:targetNode <urn:a>,<urn:b>;sh:path <urn:p>;\
         sh:class <urn:C>,<urn:D>;sh:nodeKind sh:IRI.",
        RdfFormat::Turtle,
        None,
    )
    .unwrap();
    let program = CompiledShapes::compile(shapes, false).unwrap();
    let data = parse(
        "<urn:a> <urn:p> 'plain'. <urn:b> <urn:p> 'plain'.",
        RdfFormat::Turtle,
        None,
    )
    .unwrap();
    let report = program
        .validate_readonly(
            &data,
            Options {
                abort_on_first: true,
                ..Options::default()
            },
        )
        .unwrap();
    let components: Vec<_> = report
        .graph
        .iter()
        .filter(|q| q.predicate == iri(SH, "sourceConstraintComponent").as_ref())
        .map(|q| q.object.into_owned())
        .collect();
    assert_eq!(components.len(), 4);
    assert!(
        components
            .iter()
            .all(|c| c == &iri(SH, "ClassConstraintComponent").into())
    );
}

#[test]
fn selected_focus_existing_layouts_are_observed() {
    println!(
        "FOCUS_LAYOUT Evaluation={} EvaluationBatch={} Violation={} ReportTerm={} ReportTerms={} ObservedValues={} ValueSlot={} BooleanObservation={} SelectedSlot={}",
        std::mem::size_of::<Evaluation>(),
        std::mem::size_of::<EvaluationBatch>(),
        std::mem::size_of::<Violation>(),
        std::mem::size_of::<ReportTerm>(),
        std::mem::size_of::<ReportTerms>(),
        std::mem::size_of::<crate::observed_values::ObservedValues>(),
        std::mem::size_of::<crate::observed_values::ValueSlot<'_>>(),
        std::mem::size_of::<crate::BooleanObservation>(),
        std::mem::size_of::<crate::SelectedSlot>()
    );
    // The test-only lookup counter adds eight bytes to the 48-byte production holder.
    assert_eq!(
        std::mem::size_of::<crate::observed_values::ObservedValues>(),
        56
    );
    assert_eq!(
        std::mem::size_of::<crate::observed_values::ValueSlot<'_>>(),
        16
    );
    assert_eq!(std::mem::size_of::<crate::BooleanObservation>(), 32);
}
