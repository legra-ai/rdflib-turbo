//! Node-kind report messages observed from pySHACL 0.40.1.
use super::*;
use oxigraph::io::RdfFormat;

#[test]
fn node_kind_messages_match_all_six_reference_kinds() {
    for (kind, value) in [
        ("IRI", "\"text\""),
        ("BlankNode", "<urn:o>"),
        ("Literal", "<urn:o>"),
        ("BlankNodeOrIRI", "\"text\""),
        ("BlankNodeOrLiteral", "<urn:o>"),
        ("IRIOrLiteral", "_:value"),
    ] {
        for custom in [false, true] {
            let message = if custom {
                "sh:message \"caller message\";"
            } else {
                ""
            };
            let shapes = crate::IndexedShapesSource::parse_graph(
                &format!(
                    "@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:S> a sh:PropertyShape; sh:targetNode <urn:s>; sh:path <urn:p>; {message} sh:nodeKind sh:{kind}."
                ),
                RdfFormat::Turtle,
                None,
            )
            .unwrap();
            let program = CompiledShapes::compile(shapes, false).unwrap();
            let mut data = crate::parse_graph(
                &format!("<urn:s> <urn:p> {value}."),
                RdfFormat::Turtle,
                None,
            )
            .unwrap();
            let report = program.validate(&mut data, Options::default()).unwrap();
            assert!(!report.conforms, "{kind}");
            let expected = if custom {
                "caller message".to_owned()
            } else {
                format!("Value is not of Node Kind sh:{kind}")
            };
            let messages: Vec<_> = report
                .graph
                .iter()
                .filter(|quad| quad.predicate == iri(SH, "resultMessage").as_ref())
                .map(|quad| match quad.object {
                    TermRef::Literal(value) => value.value().to_owned(),
                    _ => panic!("result message must be a Literal"),
                })
                .collect();
            assert_eq!(messages, vec![expected.clone()], "{kind}, custom={custom}");
            assert!(report.text.contains(&expected), "{}", report.text);
        }
    }
}
