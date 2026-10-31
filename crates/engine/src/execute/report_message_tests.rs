use super::*;
use oxigraph::io::RdfFormat;

#[test]
fn unique_language_reports_keep_original_default_and_custom_messages() {
    // Original pySHACL 0.40.1 emits one message per repeated language;
    // explicit sh:message terms retain their language and replace the default.
    for (values, count) in [
        ("\"HI\"@en, \"Hi\"@en", 1),
        ("\"HI\"@en, \"Hi\"@en, \"BONJOUR\"@fr, \"Bonjour\"@fr", 2),
    ] {
        for custom in [false, true] {
            let suffix = if custom {
                "; sh:message \"Provided message\"@en"
            } else {
                ""
            };
            let shapes = crate::IndexedShapesSource::parse_graph(
                &format!(
                    "@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:S> a sh:PropertyShape; sh:targetNode <urn:s>; sh:path <urn:p>; sh:uniqueLang true {suffix}."
                ),
                RdfFormat::Turtle,
                None,
            )
            .unwrap();
            let mut data = parse(
                &format!("<urn:s> <urn:p> {values}."),
                RdfFormat::Turtle,
                None,
            )
            .unwrap();
            let report = CompiledShapes::compile(shapes, false)
                .unwrap()
                .validate(&mut data, Options::default())
                .unwrap();
            assert!(!report.conforms);
            let message = if custom {
                Literal::new_language_tagged_literal("Provided message", "en").unwrap()
            } else {
                Literal::new_simple_literal("More than one String shares the same Language")
            };
            let messages: Vec<_> = report
                .graph
                .iter()
                .filter(|quad| quad.predicate == iri(SH, "resultMessage").as_ref())
                .map(|quad| quad.object.to_string())
                .collect();
            assert_eq!(messages, vec![message.to_string(); count]);
            assert_eq!(report.graph.len(), 2 + 8 * count);
            let description = format!(
                "Constraint Violation in UniqueLangConstraintComponent ({SH}UniqueLangConstraintComponent):\n\tSeverity: sh:Violation\n\tSource Shape: <urn:S>\n\tFocus Node: <urn:s>\n\tResult Path: <urn:p>\n\tMessage: {}\n",
                message.value()
            );
            assert_eq!(
                report.text,
                format!(
                    "Validation Report\nConforms: False\nResults ({count}):\n{}",
                    description.repeat(count)
                )
            );
        }
    }
}

#[test]
fn scalar_shape_messages_render_values_without_changing_rdf_terms() {
    let mut data = parse(
        "<urn:s> <urn:p> \"HI\"@en, \"Hi\"@en.",
        RdfFormat::Turtle,
        None,
    )
    .unwrap();
    for (lexical, datatype, printed) in [
        ("true", "boolean", "True"),
        ("1", "boolean", "True"),
        ("false", "boolean", "False"),
        ("0", "boolean", "False"),
        ("003", "integer", "3"),
        ("+003", "integer", "3"),
        ("-0", "integer", "0"),
        (
            "999999999999999999999999999999999999",
            "integer",
            "999999999999999999999999999999999999",
        ),
        ("003", "int", "3"),
        ("003", "string", "003"),
        ("001.2300", "decimal", "1.2300"),
        ("-0.00", "decimal", "-0.00"),
        ("+0.0000", "decimal", "0.0000"),
        ("0.000001", "decimal", "0.000001"),
        ("0.0000001", "decimal", "1E-7"),
        ("-000.0000010", "decimal", "-0.0000010"),
        (
            "100000000000000000000.0",
            "decimal",
            "100000000000000000000.0",
        ),
        ("1E+3", "decimal", "1E+3"),
        ("1.00E+3", "decimal", "1.00E+3"),
        ("1E-7", "decimal", "1E-7"),
        ("1.0000000E-7", "decimal", "1.0000000E-7"),
        ("0E+3", "decimal", "0E+3"),
        ("-0E-9", "decimal", "-0E-9"),
        ("001.2300", "string", "001.2300"),
        ("1.0E+03", "double", "1000.0"),
        ("0", "double", "0.0"),
        ("-0", "double", "-0.0"),
        ("-0.0", "double", "-0.0"),
        ("1e15", "double", "1000000000000000.0"),
        ("1e16", "double", "1e+16"),
        ("1e20", "double", "1e+20"),
        ("1e21", "double", "1e+21"),
        ("1e-4", "double", "0.0001"),
        ("1e-5", "double", "1e-05"),
        ("1e-6", "double", "1e-06"),
        ("1e-7", "double", "1e-07"),
        (
            "1.7976931348623157e308",
            "double",
            "1.7976931348623157e+308",
        ),
        (
            "2.2250738585072014e-308",
            "double",
            "2.2250738585072014e-308",
        ),
        ("5e-324", "double", "5e-324"),
        ("1.2345678901234567", "double", "1.2345678901234567"),
        ("0.10000000000000001", "double", "0.1"),
        ("9007199254740993", "double", "9007199254740992.0"),
        ("NaN", "double", "nan"),
        ("-NaN", "double", "nan"),
        ("INF", "double", "inf"),
        ("-INF", "double", "-inf"),
        ("1e309", "double", "inf"),
        ("1.0E+03", "float", "1000.0"),
        ("1.2345678901234567", "float", "1.2345678901234567"),
        ("1.0E+03", "string", "1.0E+03"),
        ("true", "string", "true"),
        ("false", "string", "false"),
    ] {
        let shapes = crate::IndexedShapesSource::parse_graph(
            &format!(
                "@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:S> a sh:PropertyShape; sh:targetNode <urn:s>; sh:path <urn:p>; sh:uniqueLang true; sh:message \"{lexical}\"^^<{XSD}{datatype}>."
            ),
            RdfFormat::Turtle,
            None,
        )
        .unwrap();
        let report = CompiledShapes::compile(shapes, false)
            .unwrap()
            .validate(&mut data, Options::default())
            .unwrap();
        assert!(!report.conforms);
        assert_eq!(report.graph.len(), 10);
        let messages: Vec<_> = report
            .graph
            .iter()
            .filter(|quad| quad.predicate == iri(SH, "resultMessage").as_ref())
            .map(|quad| quad.object.to_string())
            .collect();
        assert_eq!(
            messages,
            [Literal::new_typed_literal(lexical, iri(XSD, datatype)).to_string()]
        );
        assert!(
            report.text.ends_with(&format!("\tMessage: {printed}\n")),
            "{}",
            report.text
        );
    }
}

#[test]
fn expression_reports_keep_original_default_custom_and_source_constraint_terms() {
    // Pinned original pySHACL 0.40.1: the default is constant, and custom
    // messages replace it. False-like source terms remain in the RDF report;
    // their separate human-description truthiness frontier is not changed here.
    for (expression, printed) in [
        ("false", None),
        ("0", None),
        ("\"\"", None),
        ("1", Some("Literal(\"1\", datatype=xsd:integer)")),
        ("\"not-true\"", Some("Literal(\"not-true\")")),
        ("<urn:not-true>", Some("<urn:not-true>")),
    ] {
        for custom in [false, true] {
            let suffix = if custom {
                "; sh:message \"custom expression\"@en"
            } else {
                ""
            };
            let shapes = crate::IndexedShapesSource::parse_graph(
                &format!("@prefix sh:<{SH}>. <urn:S> sh:targetNode <urn:f>; sh:expression {expression}{suffix}."),
                RdfFormat::Turtle,
                None,
            ).unwrap();
            let mut data = Dataset::new();
            let report = CompiledShapes::compile(shapes, true)
                .unwrap()
                .validate(&mut data, Options::default())
                .unwrap();
            assert!(!report.conforms);
            assert!(data.is_empty());
            let message = if custom {
                "\"custom expression\"@en"
            } else {
                "\"Expression evaluation generated constraint did not return true.\""
            };
            let edge = report
                .graph
                .iter()
                .find(|q| q.predicate == iri(SH, "result").as_ref())
                .unwrap();
            let expected = parse(&format!(
                "@prefix sh:<{SH}>. @prefix rdf:<{RDF}>. {} a sh:ValidationReport; sh:conforms false; sh:result {}. {} a sh:ValidationResult; sh:sourceConstraintComponent sh:ExpressionConstraintComponent; sh:sourceShape <urn:S>; sh:resultSeverity sh:Violation; sh:focusNode <urn:f>; sh:value <urn:f>; sh:sourceConstraint {expression}; sh:resultMessage {message}.",
                edge.subject, edge.object, edge.object
            ), RdfFormat::Turtle, None).unwrap();
            assert_eq!(
                report.graph.len(),
                expected.len(),
                "{expression}, custom={custom}"
            );
            assert!(
                expected.iter().all(|q| report.graph.contains(q)),
                "{expression}, custom={custom}: {:?}",
                report.graph
            );
            if let Some(printed) = printed {
                let message = if custom {
                    "custom expression"
                } else {
                    "Expression evaluation generated constraint did not return true."
                };
                assert_eq!(
                    report.text,
                    format!(
                        "Validation Report\nConforms: False\nResults (1):\nConstraint Violation in ExpressionConstraintComponent ({SH}ExpressionConstraintComponent):\n\tSeverity: sh:Violation\n\tSource Shape: <urn:S>\n\tFocus Node: <urn:f>\n\tValue Node: <urn:f>\n\tSource Constraint: {printed}\n\tMessage: {message}\n"
                    )
                );
            }
        }
    }
}
