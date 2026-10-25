use super::*;
use oxigraph::io::RdfFormat;

fn rules() -> CompiledShapes {
    CompiledShapes::compile(crate::IndexedShapesSource::parse_graph("@prefix sh: <http://www.w3.org/ns/shacl#>. <urn:S> sh:targetNode <urn:s>; sh:rule [a sh:TripleRule; sh:subject sh:this; sh:predicate <urn:derived>; sh:object true].", RdfFormat::Turtle, None).unwrap(), true).unwrap()
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
