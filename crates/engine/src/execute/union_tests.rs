use super::*;
use oxigraph::io::RdfFormat;

#[test]
fn validation_without_active_rules_never_materializes_its_validation_union() {
    let shapes = crate::IndexedShapesSource::parse_graph("@prefix sh:<http://www.w3.org/ns/shacl#> . <urn:S> sh:targetNode <urn:s>; sh:path <urn:p>; sh:minCount 1.", RdfFormat::Turtle, None).unwrap();
    let program = CompiledShapes::compile(shapes, true).unwrap();
    let mut data = parse_dataset("<urn:g> { <urn:s> <urn:p> 1 }", RdfFormat::TriG, None).unwrap();
    UNION_BUILDS.with(|count| count.set(0));
    assert!(
        program
            .validate(&mut data, Options::default())
            .unwrap()
            .conforms
    );
    assert_eq!(UNION_BUILDS.with(std::cell::Cell::get), 0);
    assert_eq!(data.len(), 1);
    assert!(!data.iter().any(|q| q.graph_name.is_default_graph()));
    assert!(
        program
            .expand_rules(
                &mut data,
                Options {
                    max_rule_iterations: 0,
                    ..Options::default()
                }
            )
            .is_err()
    );
}
