//! Exact node-component messages observed from pySHACL 0.40.1.
use super::*;
use oxigraph::io::RdfFormat;

#[test]
fn node_messages_preserve_single_multiple_nested_and_custom_shape_forms() {
    for (name, body, expected) in [
        (
            "named",
            "<urn:Outer> sh:targetNode <urn:s>; sh:node <urn:C>. <urn:C> sh:class <urn:Class>.",
            vec!["Value does not conform to Shape <urn:C>. See details for more information."],
        ),
        (
            "multiple",
            "<urn:Outer> sh:targetNode <urn:s>; sh:node <urn:C>, <urn:D>. <urn:C> sh:class <urn:Class>. <urn:D> sh:class <urn:Class>.",
            vec![
                "Value must conform to every Shape in ('<urn:C>', '<urn:D>'). See details for more information.",
                "Value must conform to every Shape in ('<urn:C>', '<urn:D>'). See details for more information.",
            ],
        ),
        (
            "prefixed",
            "@prefix ex:<http://example.org/>. <urn:Outer> sh:targetNode <urn:s>; sh:node ex:C. ex:C sh:class <urn:Class>.",
            vec!["Value does not conform to Shape ex:C. See details for more information."],
        ),
        (
            "blank",
            "<urn:Outer> sh:targetNode <urn:s>; sh:node [sh:class <urn:Class>].",
            vec![
                "Value does not conform to Shape [ sh:class <urn:Class> ]. See details for more information.",
            ],
        ),
        (
            "custom",
            "<urn:Outer> sh:targetNode <urn:s>; sh:node <urn:C>; sh:message \"caller message\". <urn:C> sh:class <urn:Class>.",
            vec!["caller message"],
        ),
        (
            "nested",
            "<urn:Outer> sh:targetNode <urn:s>; sh:node <urn:Middle>. <urn:Middle> sh:node <urn:C>. <urn:C> sh:class <urn:Class>.",
            vec![
                "Value does not conform to Shape <urn:Middle>. See details for more information.",
                "Value does not conform to Shape <urn:C>. See details for more information.",
            ],
        ),
    ] {
        let shapes = crate::IndexedShapesSource::parse_graph(
            &format!("@prefix sh:<http://www.w3.org/ns/shacl#>. {body}"),
            RdfFormat::Turtle,
            None,
        )
        .unwrap();
        let mut namespaces = Vec::new();
        if name == "prefixed" {
            namespaces.push(("ex".into(), "http://example.org/".into()));
        }
        let program =
            CompiledShapes::compile_with_namespaces(shapes, false, false, &namespaces).unwrap();
        let mut data =
            crate::parse_graph("<urn:s> <urn:p> <urn:o>.", RdfFormat::Turtle, None).unwrap();
        let report = program.validate(&mut data, Options::default()).unwrap();
        assert!(!report.conforms, "{name}");
        let subjects: Vec<_> = report
            .graph
            .iter()
            .filter(|quad| {
                quad.predicate == iri(SH, "sourceConstraintComponent").as_ref()
                    && quad.object == iri(SH, "NodeConstraintComponent").as_ref().into()
            })
            .map(|quad| quad.subject.into_owned())
            .collect();
        let mut messages: Vec<_> = report
            .graph
            .iter()
            .filter(|quad| {
                quad.predicate == iri(SH, "resultMessage").as_ref()
                    && subjects
                        .iter()
                        .any(|subject| quad.subject == subject.as_ref())
            })
            .map(|quad| match quad.object {
                TermRef::Literal(value) => value.value().to_owned(),
                _ => panic!("result message is not a Literal"),
            })
            .collect();
        messages.sort();
        let mut expected: Vec<_> = expected.into_iter().map(str::to_owned).collect();
        expected.sort();
        assert_eq!(messages, expected, "{name}");
        for message in messages {
            assert!(report.text.contains(&message), "{name}: {}", report.text);
        }
    }
}
