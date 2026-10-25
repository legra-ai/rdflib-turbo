//! SHACL 1.2 Core 2026-10-14 draft coverage, separate from pySHACL parity.

use oxigraph::io::RdfFormat;
use oxigraph::model::*;
use pyshacl_turbo_engine::{CompiledShapes, IndexedShapesSource, Options};

fn shape(enabled: &str) -> IndexedShapesSource {
    IndexedShapesSource::parse_graph(
        &format!(
            "@prefix sh: <http://www.w3.org/ns/shacl#>. \
             <urn:shape> a sh:NodeShape; sh:targetNode <urn:focus>; \
             sh:property [ sh:path <urn:label>; sh:singleLine {enabled} ]."
        ),
        RdfFormat::Turtle,
        None,
    )
    .unwrap()
}

#[test]
fn single_line_checks_all_four_draft_characters_and_preserves_false() {
    for enabled in [true, false] {
        let compiled = CompiledShapes::compile(shape(&enabled.to_string()), false).unwrap();
        for text in ["a\nb", "a\u{000b}b", "a\u{000c}b", "a\rb", "a\r\nb"] {
            let mut data = Dataset::new();
            data.insert(&Quad::new(
                NamedNode::new("urn:focus").unwrap(),
                NamedNode::new("urn:label").unwrap(),
                Literal::new_simple_literal(text),
                GraphName::DefaultGraph,
            ));
            let report = compiled.validate(&mut data, Options::default()).unwrap();
            assert_eq!(report.conforms, !enabled, "{text:?}, enabled={enabled}");
            if enabled {
                assert!(report.graph.iter().any(|quad| {
                    quad.predicate.as_str()
                        == "http://www.w3.org/ns/shacl#sourceConstraintComponent"
                        && quad.object
                            == TermRef::NamedNode(
                                NamedNodeRef::new(
                                    "http://www.w3.org/ns/shacl#SingleLineConstraintComponent",
                                )
                                .unwrap(),
                            )
                }));
            }
        }
    }
}

#[test]
fn single_line_ignores_non_literals_and_other_unicode_separators() {
    let compiled = CompiledShapes::compile(shape("true"), false).unwrap();
    for value in [
        Term::NamedNode(NamedNode::new("urn:value").unwrap()),
        Term::BlankNode(BlankNode::new("value").unwrap()),
        Term::Literal(Literal::new_simple_literal("a\u{0085}b\u{2028}c\u{2029}d")),
    ] {
        let mut data = Dataset::new();
        data.insert(&Quad::new(
            NamedNode::new("urn:focus").unwrap(),
            NamedNode::new("urn:label").unwrap(),
            value,
            GraphName::DefaultGraph,
        ));
        assert!(
            compiled
                .validate(&mut data, Options::default())
                .unwrap()
                .conforms
        );
    }
}

#[test]
fn single_line_rejects_non_boolean_and_repeated_parameters() {
    for value in ["\"true\"", "1", "true, false"] {
        assert!(
            CompiledShapes::compile(shape(value), false).is_err(),
            "{value}"
        );
    }
}
