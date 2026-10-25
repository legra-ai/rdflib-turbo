use pyshacl_turbo_engine::oxigraph::io::RdfFormat;
use pyshacl_turbo_engine::oxigraph::model::*;
use pyshacl_turbo_engine::{CompiledShapes, Options, parse_graph};

const DATA: &str = include_str!("../../../tests/fixtures/pipeline-data.ttl");
const SHAPES: &str = include_str!("../../../tests/fixtures/pipeline-shapes.ttl");

#[test]
fn rule_delta_retains_only_new_statements_and_preserves_named_input() {
    let shapes = pyshacl_turbo_engine::IndexedShapesSource::parse_graph(
        "@prefix sh:<http://www.w3.org/ns/shacl#> . <urn:S> sh:targetSubjectsOf <urn:p>; sh:rule [a sh:TripleRule; sh:subject sh:this; sh:predicate <urn:q>; sh:object [sh:path <urn:p>]].",
        RdfFormat::Turtle, None,
    ).unwrap();
    let program = CompiledShapes::compile(shapes, true).unwrap();
    let mut data = pyshacl_turbo_engine::parse_dataset(
        "<urn:g> { _:s <urn:p> \"01\"^^<http://www.w3.org/2001/XMLSchema#integer> }",
        RdfFormat::TriG,
        None,
    )
    .unwrap();
    let original = data.clone();
    let delta = program
        .expand_rules_delta(&mut data, Options::default())
        .unwrap();
    assert_eq!(delta.graph.len(), 1);
    assert_eq!(data.len(), 2);
    assert!(original.iter().all(|q| data.contains(q)));
    let added = delta.graph.iter().next().unwrap();
    assert_eq!(added.predicate.as_str(), "urn:q");
    assert_eq!(
        added.object.to_string(),
        "\"01\"^^<http://www.w3.org/2001/XMLSchema#integer>"
    );
    assert!(added.graph_name.is_default_graph());
    let repeated = program
        .expand_rules_delta(&mut data, Options::default())
        .unwrap();
    assert!(repeated.graph.is_empty());
}

#[test]
fn native_pipeline_derives_and_validates_without_python() {
    let program = CompiledShapes::compile(
        pyshacl_turbo_engine::IndexedShapesSource::parse_graph(SHAPES, RdfFormat::Turtle, None)
            .unwrap(),
        true,
    )
    .unwrap();
    let mut data = parse_graph(DATA, RdfFormat::Turtle, None).unwrap();
    let report = program.validate(&mut data, Options::default()).unwrap();
    assert!(!report.conforms);
    let eligible = NamedNode::new("https://example.com/eligible").unwrap();
    let alice = NamedNode::new("https://example.com/alice").unwrap();
    let bob = NamedNode::new("https://example.com/bob").unwrap();
    assert!(data.contains(QuadRef::new(
        &alice,
        &eligible,
        LiteralRef::new_typed_literal(
            "true",
            NamedNodeRef::new("http://www.w3.org/2001/XMLSchema#boolean").unwrap()
        ),
        GraphNameRef::DefaultGraph
    )));
    let focuses: Vec<_> = report
        .graph
        .quads_for_pattern(
            None,
            Some(NamedNodeRef::new("http://www.w3.org/ns/shacl#focusNode").unwrap()),
            None,
            None,
        )
        .map(|q| q.object.into_owned())
        .collect();
    assert_eq!(focuses, vec![Term::from(bob)]);
}

#[test]
fn compiled_program_is_shareable_and_data_is_independent() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<CompiledShapes>();
    let program = CompiledShapes::compile(
        pyshacl_turbo_engine::IndexedShapesSource::parse_graph(SHAPES, RdfFormat::Turtle, None)
            .unwrap(),
        true,
    )
    .unwrap();
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(|| {
                    let mut data = parse_graph(DATA, RdfFormat::Turtle, None).unwrap();
                    program
                        .validate(&mut data, Options::default())
                        .unwrap()
                        .conforms
                })
            })
            .collect();
        for handle in handles {
            assert!(!handle.join().unwrap());
        }
    });
}

#[test]
fn named_data_graph_survives_validation_without_being_copied_to_default() {
    let program = CompiledShapes::compile(
        pyshacl_turbo_engine::IndexedShapesSource::parse_graph(SHAPES, RdfFormat::Turtle, None)
            .unwrap(),
        true,
    )
    .unwrap();
    let mut data = parse_graph(DATA, RdfFormat::Turtle, None).unwrap();
    data.insert(&Quad::new(
        NamedNode::new("https://example.com/s").unwrap(),
        NamedNode::new("https://example.com/p").unwrap(),
        Literal::from(1),
        NamedNode::new("https://example.com/graph").unwrap(),
    ));
    assert!(
        !program
            .validate(&mut data, Options::default())
            .unwrap()
            .conforms
    );
    let s = NamedNode::new("https://example.com/s").unwrap();
    assert_eq!(data.quads_for_subject(s.as_ref()).count(), 1);
    assert!(
        !data
            .quads_for_subject(s.as_ref())
            .next()
            .unwrap()
            .graph_name
            .is_default_graph()
    );
}

#[test]
fn oxigraph_store_validates_without_serialization_and_commits_explicitly() {
    let program = CompiledShapes::compile(
        pyshacl_turbo_engine::IndexedShapesSource::parse_graph(SHAPES, RdfFormat::Turtle, None)
            .unwrap(),
        true,
    )
    .unwrap();
    let store = pyshacl_turbo_engine::oxigraph::store::Store::new().unwrap();
    let data = parse_graph(DATA, RdfFormat::Turtle, None).unwrap();
    store.extend(data.iter().map(|q| q.into_owned())).unwrap();
    let before = store.len().unwrap();
    assert!(
        !program
            .validate_store(&store, Options::default(), false)
            .unwrap()
            .conforms
    );
    assert_eq!(store.len().unwrap(), before);
    assert!(
        !program
            .validate_store(&store, Options::default(), true)
            .unwrap()
            .conforms
    );
    assert!(store.len().unwrap() > before);
}

#[test]
fn literal_identity_survives_storage() {
    let source = r#"@prefix ex: <https://example.com/>.
        @prefix xsd: <http://www.w3.org/2001/XMLSchema#>.
        ex:s ex:p "01"^^xsd:integer, "1"^^xsd:integer, "300"^^xsd:byte, "4.0"^^xsd:decimal."#;
    let data = parse_graph(source, RdfFormat::Turtle, None).unwrap();
    let p = NamedNodeRef::new("https://example.com/p").unwrap();
    let objects: Vec<_> = data
        .quads_for_pattern(None, Some(p), None, None)
        .map(|q| q.object.into_owned())
        .collect();
    assert_eq!(
        objects.len(),
        4,
        "Distinct RDF terms must not collapse by numeric value"
    );
    assert!(
        objects.contains(
            &Literal::new_typed_literal(
                "300",
                NamedNode::new("http://www.w3.org/2001/XMLSchema#byte").unwrap()
            )
            .into()
        )
    );
}

#[test]
fn plain_and_typed_strings_survive_dataset_round_trip() {
    use pyshacl_turbo_engine::{parse_dataset, serialize_graph};
    let source = r#"<urn:s> <urn:p> "value" .
        <urn:s> <urn:p> "value"^^<http://www.w3.org/2001/XMLSchema#string> ."#;
    let data = parse_dataset(source, RdfFormat::NQuads, None).unwrap();
    let plain = Quad::new(
        NamedNode::new("urn:s").unwrap(),
        NamedNode::new("urn:p").unwrap(),
        Literal::new_simple_literal("value"),
        GraphName::DefaultGraph,
    );
    let mut typed = plain.clone();
    typed.object = Literal::new_typed_literal(
        "value",
        NamedNode::new("http://www.w3.org/2001/XMLSchema#string").unwrap(),
    )
    .into();
    assert_eq!(data.len(), 2);
    assert!(data.contains(&plain) && data.contains(&typed));
    let serialized = serialize_graph(&data).unwrap();
    assert!(serialized.contains("^^<http://www.w3.org/2001/XMLSchema#string>"));
    let mut restored = parse_dataset(&serialized, RdfFormat::NQuads, None).unwrap();
    assert_eq!(data, restored);
    assert!(restored.remove(&plain));
    assert!(!restored.contains(&plain));
    assert!(restored.contains(&typed));
}

#[test]
fn sparql_optimizer_accounts_for_prebound_variables() {
    use pyshacl_turbo_engine::oxigraph::sparql::{QueryResults, SparqlEvaluator};
    let data = Dataset::new();
    for value in [
        Term::from(BlankNode::default()),
        Term::from(NamedNode::new("https://example.com/focus").unwrap()),
    ] {
        let query = SparqlEvaluator::new().parse_query(
            "SELECT ?this ?that WHERE { BIND(?this AS ?that) FILTER(BOUND(?this) && BOUND(?that)) }"
        ).unwrap();
        let QueryResults::Solutions(mut solutions) = query
            .on_queryable_dataset(&data)
            .substitute_variable(Variable::new("this").unwrap(), value.clone())
            .execute()
            .unwrap()
        else {
            panic!("Expected solutions");
        };
        let row = solutions
            .next()
            .expect("Prebound values must survive optimization")
            .unwrap();
        assert_eq!(row.get("this"), Some(&value));
        assert_eq!(row.get("that"), Some(&value));
        assert!(solutions.next().is_none());
    }
}
