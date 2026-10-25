//! Disabled SHACL-AF rule bodies are not part of the core program.
use pyshacl_turbo_engine::oxigraph::io::RdfFormat;
use pyshacl_turbo_engine::oxigraph::model::*;
use pyshacl_turbo_engine::{CompiledShapes, IndexedShapesSource, Options};

fn program(rule: &str, advanced: bool) -> pyshacl_turbo_engine::Result<CompiledShapes> {
    let source = IndexedShapesSource::parse_graph(
        &format!(
            "@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:S> sh:targetNode <urn:focus>; sh:rule {rule}."
        ),
        RdfFormat::Turtle,
        None,
    )?;
    CompiledShapes::compile(source, advanced)
}

#[test]
fn core_validation_ignores_disabled_rules_and_their_conditions() {
    for rule in [
        "[a sh:TripleRule; sh:subject sh:this; sh:predicate <urn:added>; sh:object true]",
        "[a sh:TripleRule]",
        "[a sh:SPARQLRule; sh:construct 'not a query']",
        "[a <urn:UnknownRule>; sh:unknown true]",
        "[a sh:TripleRule; sh:condition [sh:nodeKind <urn:UnknownKind>]]",
        "[a sh:TripleRule; sh:object [sh:filterShape [sh:nodeKind <urn:UnknownKind>]]]",
        "[a sh:TripleRule; sh:object [sh:path 'not a path']]",
    ] {
        let program = program(rule, false).unwrap();
        let mut data = Dataset::new();
        assert!(
            program
                .validate(&mut data, Options::default())
                .unwrap()
                .conforms
        );
        assert!(data.is_empty(), "inactive rule changed data: {rule}");
    }
}

#[test]
fn core_validation_ignores_disabled_targets_expressions_and_metadata_paths() {
    for annotation in [
        "sh:target [a <urn:UnknownTarget>]",
        "sh:expression [sh:unknown true]",
    ] {
        let source = IndexedShapesSource::parse_graph(
            &format!(
                "@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:S> sh:targetNode <urn:focus>; {annotation}."
            ),
            RdfFormat::Turtle,
            None,
        ).unwrap();
        let program = CompiledShapes::compile(source, false).unwrap();
        assert!(
            program
                .validate(&mut Dataset::new(), Options::default())
                .unwrap()
                .conforms
        );
    }
    for advanced in [false, true] {
        let source = IndexedShapesSource::parse_graph(
            "@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:metadata> sh:path 'not a path'.",
            RdfFormat::Turtle,
            None,
        )
        .unwrap();
        assert!(CompiledShapes::compile(source, advanced).is_ok());
    }
    let source = IndexedShapesSource::parse_graph(
        "@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:metadata> sh:target [a <urn:UnknownTarget>].",
        RdfFormat::Turtle,
        None,
    ).unwrap();
    assert!(CompiledShapes::compile(source, true).is_ok());
}

#[test]
fn enabled_rules_are_still_checked_and_executed() {
    assert!(program("[a sh:TripleRule]", true).is_err());
    assert!(program("[a sh:SPARQLRule; sh:construct 'not a query']", true).is_err());
    let program = program(
        "[a sh:TripleRule; sh:subject sh:this; sh:predicate <urn:added>; sh:object true]",
        true,
    )
    .unwrap();
    let mut data = Dataset::new();
    assert!(
        program
            .validate(&mut data, Options::default())
            .unwrap()
            .conforms
    );
    assert_eq!(data.len(), 1);
}
