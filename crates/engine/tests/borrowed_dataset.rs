use pyshacl_turbo_engine::oxigraph::io::RdfFormat;
use pyshacl_turbo_engine::{CompiledShapes, Options, parse_graph};

#[test]
fn validation_borrows_the_callers_dataset_and_preserves_the_full_report() {
    let data = parse_graph("<urn:s> <urn:p> 1 .", RdfFormat::Turtle, None).unwrap();
    let shapes = pyshacl_turbo_engine::IndexedShapesSource::parse_graph(
        "@prefix sh: <http://www.w3.org/ns/shacl#> .
         <urn:S> sh:targetNode <urn:s>; sh:property [sh:path <urn:q>; sh:minCount 1].",
        RdfFormat::Turtle,
        None,
    )
    .unwrap();
    let program = CompiledShapes::compile(shapes, false).unwrap();
    let borrowed = program
        .validate_readonly(&data, Options::default())
        .unwrap();
    let ordinary = program
        .validate(&mut data.clone(), Options::default())
        .unwrap();
    assert!(!borrowed.conforms);
    assert_eq!(borrowed.text, ordinary.text);
    assert_eq!(borrowed.graph.len(), ordinary.graph.len());
    assert_eq!(data.len(), 1);
    let invalid = Options {
        max_rule_iterations: 0,
        ..Options::default()
    };
    assert!(
        program
            .validate_readonly(&data, invalid)
            .err()
            .unwrap()
            .to_string()
            .contains("positive")
    );
}

#[test]
fn readonly_validation_rejects_active_rules_before_execution() {
    let data = parse_graph("<urn:s> <urn:p> 1 .", RdfFormat::Turtle, None).unwrap();
    let shapes = pyshacl_turbo_engine::IndexedShapesSource::parse_graph(
        "@prefix sh: <http://www.w3.org/ns/shacl#> .
         <urn:S> sh:targetNode <urn:s>; sh:rule [a sh:TripleRule;
         sh:subject sh:this; sh:predicate <urn:q>; sh:object 2].",
        RdfFormat::Turtle,
        None,
    )
    .unwrap();
    let program = CompiledShapes::compile(shapes, true).unwrap();
    assert!(program.has_active_rules(&Options::default()));
    let failure = program
        .validate_readonly(&data, Options::default())
        .err()
        .unwrap();
    assert!(failure.to_string().contains("active rules"));
    assert_eq!(data.len(), 1);
}
