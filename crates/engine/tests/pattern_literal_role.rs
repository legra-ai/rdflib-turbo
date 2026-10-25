//! Ordinary language patterns share the string-pattern compiler and reports.
use pyshacl_turbo_engine::oxigraph::io::RdfFormat;
use pyshacl_turbo_engine::oxigraph::model::*;
use pyshacl_turbo_engine::{CompiledShapes, IndexedShapesSource, Options};

fn check(pattern: &str, focused: bool) {
    let target = if focused {
        "; sh:targetNode <urn:focus>"
    } else {
        ""
    };
    let text = format!(
        "@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:S> a sh:PropertyShape; sh:path <urn:p>; sh:pattern {pattern}{target}."
    );
    let source = IndexedShapesSource::parse_graph(&text, RdfFormat::Turtle, None).unwrap();
    let program = CompiledShapes::compile(source, false).unwrap();
    let mut data = Dataset::new();
    for value in ["abc", "no"] {
        data.insert(&Quad::new(
            NamedNode::new_unchecked("urn:focus"),
            NamedNode::new_unchecked("urn:p"),
            Literal::new_simple_literal(value),
            GraphName::DefaultGraph,
        ));
    }
    let report = program
        .validate_readonly(&data, Options::default())
        .unwrap();
    assert_eq!(report.conforms, !focused);
    let failed_values: Vec<_> = report
        .graph
        .iter()
        .filter(|quad| quad.predicate.as_str() == "http://www.w3.org/ns/shacl#value")
        .map(|quad| quad.object.into_owned())
        .collect();
    if focused {
        assert_eq!(
            failed_values,
            vec![Literal::new_simple_literal("no").into()]
        );
        assert!(report.text.contains("Value does not match pattern 'abc'"));
    } else {
        assert!(failed_values.is_empty());
        assert_eq!(report.text, "Validation Report\nConforms: True\n");
    }
}

#[test]
fn plain_active() {
    check("'abc'", true);
}
#[test]
fn plain_dormant() {
    check("'abc'", false);
}
#[test]
fn string_active() {
    check("'abc'^^<http://www.w3.org/2001/XMLSchema#string>", true);
}
#[test]
fn string_dormant() {
    check("'abc'^^<http://www.w3.org/2001/XMLSchema#string>", false);
}
#[test]
fn language_active() {
    check("'abc'@en", true);
}
#[test]
fn language_dormant() {
    check("'abc'@en", false);
}
