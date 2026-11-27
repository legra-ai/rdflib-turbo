//! Basic language-range validation at the RDF-to-structural-model boundary.

use oxigraph::io::RdfFormat;
use oxigraph::model::*;
use pyshacl_turbo_engine::{CompiledShapes, IndexedShapesSource, Options};

fn compile(member: &str) -> pyshacl_turbo_engine::Result<CompiledShapes> {
    let source = IndexedShapesSource::parse_graph(
        &format!(
            "@prefix sh: <http://www.w3.org/ns/shacl#>. \
             <urn:shape> a sh:NodeShape; sh:targetNode <urn:focus>; \
             sh:property [ sh:path <urn:label>; sh:languageIn ({member}) ]."
        ),
        RdfFormat::Turtle,
        None,
    )?;
    CompiledShapes::compile(source, false)
}

#[test]
fn language_in_keeps_case_insensitive_subtag_matching() {
    let compiled = compile("\"EN\"").unwrap();
    for (language, expected) in [("en", true), ("en-GB", true), ("fr", false)] {
        let mut data = Dataset::new();
        data.insert(&Quad::new(
            NamedNode::new("urn:focus").unwrap(),
            NamedNode::new("urn:label").unwrap(),
            Literal::new_language_tagged_literal("label", language).unwrap(),
            GraphName::DefaultGraph,
        ));
        assert_eq!(
            compiled
                .validate(&mut data, Options::default())
                .unwrap()
                .conforms,
            expected,
            "{language}"
        );
    }
}

#[test]
fn language_in_rejects_malformed_ranges_at_compilation() {
    for member in ["en--GB", "abcdefghi"] {
        let error = compile(&format!("\"{member}\""))
            .err()
            .expect("malformed language range must fail during compilation")
            .to_string();
        assert!(error.contains("languageIn"), "{error}");
        assert!(error.contains(member), "{error}");
    }
}

#[test]
fn language_in_accepts_basic_ranges_that_are_not_language_tags() {
    let compiled = compile("\"*\"").unwrap();
    for value in [
        Literal::new_language_tagged_literal("label", "en").unwrap(),
        Literal::new_language_tagged_literal("label", "fr").unwrap(),
        Literal::new_simple_literal("label"),
    ] {
        let expected = value.language().is_some();
        let mut data = Dataset::new();
        data.insert(&Quad::new(
            NamedNode::new("urn:focus").unwrap(),
            NamedNode::new("urn:label").unwrap(),
            value,
            GraphName::DefaultGraph,
        ));
        assert_eq!(
            compiled
                .validate(&mut data, Options::default())
                .unwrap()
                .conforms,
            expected
        );
    }
    assert!(compile("\"de-1996-1996\"").is_ok());
}
