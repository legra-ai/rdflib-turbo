//! Exact language-list messages observed from pySHACL 0.40.1.
use super::*;
use oxigraph::io::RdfFormat;

#[test]
fn language_messages_render_the_original_list_head_and_keep_custom_messages() {
    for (rule, extra, expected, custom) in [
        (
            "(\"EN\" \"fr\")",
            "",
            "String language is not in ( Literal(\"EN\") Literal(\"fr\") )",
            false,
        ),
        ("()", "", "String language is not in rdf:nil", false),
        (
            "<urn:allowed-languages>",
            "<urn:allowed-languages> rdf:first \"en\"; rdf:rest rdf:nil.",
            "String language is not in <urn:allowed-languages>",
            false,
        ),
        ("()", "", "custom language failure", true),
    ] {
        let message = if custom {
            "sh:message \"custom language failure\"@en;"
        } else {
            ""
        };
        let shapes = crate::IndexedShapesSource::parse_graph(
            &format!(
                "@prefix sh:<http://www.w3.org/ns/shacl#>. @prefix rdf:<{RDF}>. <urn:S> sh:targetNode <urn:f>; sh:path <urn:p>; {message} sh:languageIn {rule}. {extra}"
            ),
            RdfFormat::Turtle,
            None,
        )
        .unwrap();
        let program = CompiledShapes::compile(shapes, false).unwrap();
        let mut data =
            crate::parse_graph("<urn:f> <urn:p> \"value\"@de.", RdfFormat::Turtle, None).unwrap();
        let report = program.validate(&mut data, Options::default()).unwrap();
        assert!(!report.conforms, "{rule}");
        let expected_literal = if custom {
            Literal::new_language_tagged_literal(expected, "en").unwrap()
        } else {
            Literal::new_simple_literal(expected)
        };
        let messages: Vec<_> = report
            .graph
            .iter()
            .filter(|quad| quad.predicate == iri(SH, "resultMessage").as_ref())
            .map(|quad| quad.object.into_owned())
            .collect();
        assert_eq!(messages, vec![expected_literal.into()], "{rule}");
        assert_eq!(report.graph.len(), 11, "{rule}");
        assert_eq!(
            report.text,
            format!(
                "Validation Report\nConforms: False\nResults (1):\nConstraint Violation in LanguageInConstraintComponent ({SH}LanguageInConstraintComponent):\n\tSeverity: sh:Violation\n\tSource Shape: <urn:S>\n\tFocus Node: <urn:f>\n\tValue Node: Literal(\"value\", lang=de)\n\tResult Path: <urn:p>\n\tMessage: {expected}\n"
            ),
            "{rule}"
        );
    }
}
