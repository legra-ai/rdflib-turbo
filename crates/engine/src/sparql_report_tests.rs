//! Standalone SELECT reports preserve the absence of a result message.
use crate::graph::*;
use crate::{CompiledShapes, IndexedShapesSource, Options};
use oxigraph::io::RdfFormat;

#[test]
fn standalone_select_without_messages_does_not_invent_a_generic_message() {
    let shapes = IndexedShapesSource::parse_graph(
        "@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:S> sh:targetNode <urn:f>; sh:sparql <urn:Q>. <urn:Q> sh:select \"SELECT $this WHERE {}\".",
        RdfFormat::Turtle,
        None,
    )
    .unwrap();
    let program = CompiledShapes::compile(shapes, false).unwrap();
    let mut data = parse("<urn:f> <urn:p> \"value\"@de.", RdfFormat::Turtle, None).unwrap();
    let report = program.validate(&mut data, Options::default()).unwrap();
    assert!(!report.conforms);
    assert!(
        !report
            .graph
            .iter()
            .any(|quad| quad.predicate == iri(SH, "resultMessage").as_ref())
    );
    assert_eq!(report.graph.len(), 10);
    assert_eq!(
        report.text,
        format!(
            "Validation Report\nConforms: False\nResults (1):\nConstraint Violation in SPARQLConstraintComponent ({SH}SPARQLConstraintComponent):\n\tSeverity: sh:Violation\n\tSource Shape: <urn:S>\n\tFocus Node: <urn:f>\n\tValue Node: <urn:f>\n\tSource Constraint: <urn:Q>\n"
        )
    );
}

#[test]
fn standalone_select_keeps_interpolated_shape_query_and_explicit_empty_messages() {
    for (shape_message, query_message, expected) in [
        ("", "; sh:message \"Bad {$this}\"@en", "Bad urn:f"),
        ("; sh:message \"shape message\"@en", "", "shape message"),
        ("", "; sh:message \"\"", ""),
    ] {
        let shapes = IndexedShapesSource::parse_graph(
            &format!(
                "@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:S> sh:targetNode <urn:f>; sh:sparql <urn:Q> {shape_message}. <urn:Q> sh:select \"SELECT $this WHERE {{}}\" {query_message}."
            ),
            RdfFormat::Turtle,
            None,
        )
        .unwrap();
        let program = CompiledShapes::compile(shapes, false).unwrap();
        let mut data = parse("<urn:f> <urn:p> \"value\"@de.", RdfFormat::Turtle, None).unwrap();
        let report = program.validate(&mut data, Options::default()).unwrap();
        assert!(!report.conforms);
        let messages: Vec<_> = report
            .graph
            .iter()
            .filter(|quad| quad.predicate == iri(SH, "resultMessage").as_ref())
            .map(|quad| quad.object.into_owned())
            .collect();
        assert_eq!(
            messages,
            vec![oxigraph::model::Literal::new_simple_literal(expected).into()]
        );
        assert_eq!(report.graph.len(), 11);
        assert_eq!(
            report.text,
            format!(
                "Validation Report\nConforms: False\nResults (1):\nConstraint Violation in SPARQLConstraintComponent ({SH}SPARQLConstraintComponent):\n\tSeverity: sh:Violation\n\tSource Shape: <urn:S>\n\tFocus Node: <urn:f>\n\tValue Node: <urn:f>\n\tSource Constraint: <urn:Q>\n\tMessage: {expected}\n"
            )
        );
    }
}

#[test]
fn parameterized_ask_reports_keep_the_reference_generic_message_and_title() {
    let namespace = "https://example.test/sparql-message#";
    let namespaces = vec![("ex".to_owned(), namespace.to_owned())];
    for property in [false, true] {
        let kind = if property {
            "PropertyShape"
        } else {
            "NodeShape"
        };
        let path = if property { "sh:path ex:p;" } else { "" };
        let shapes = IndexedShapesSource::parse_graph(
            &format!(
                "@prefix sh:<{SH}>. @prefix ex:<{namespace}>. ex:Shape a sh:{kind}; sh:targetNode ex:focus; {path} ex:expected true. ex:Component a sh:ConstraintComponent; sh:parameter [sh:path ex:expected]; sh:validator [a sh:SPARQLAskValidator; sh:ask \"ASK {{ FILTER($value = $expected) }}\"]."
            ),
            RdfFormat::Turtle,
            None,
        )
        .unwrap();
        let program =
            CompiledShapes::compile_with_namespaces(shapes, false, true, &namespaces).unwrap();
        let mut data = parse(
            &format!("@prefix ex:<{namespace}>. ex:focus ex:p ex:value."),
            RdfFormat::Turtle,
            None,
        )
        .unwrap();
        let report = program
            .validate(
                &mut data,
                Options {
                    namespaces: namespaces.clone(),
                    ..Options::default()
                },
            )
            .unwrap();
        assert!(!report.conforms);
        let message = "Parameterised SHACL Query generated constraint validation reports.";
        let messages: Vec<_> = report
            .graph
            .iter()
            .filter(|quad| quad.predicate == iri(SH, "resultMessage").as_ref())
            .map(|quad| quad.object.into_owned())
            .collect();
        assert_eq!(
            messages,
            vec![oxigraph::model::Literal::new_simple_literal(message).into()]
        );
        assert_eq!(report.graph.len(), 10);
        let value_or_path = if property {
            "\tResult Path: ex:p\n"
        } else {
            "\tValue Node: ex:focus\n"
        };
        assert_eq!(
            report.text,
            format!(
                "Validation Report\nConforms: False\nResults (1):\nConstraint Violation in ConstraintComponent ({namespace}Component):\n\tSeverity: sh:Violation\n\tSource Shape: ex:Shape\n\tFocus Node: ex:focus\n{value_or_path}\tMessage: {message}\n"
            )
        );
    }
}
