use super::*;
use oxigraph::sparql::SparqlEvaluator;

#[test]
fn lexical_values_preserve_iris_literal_spelling_and_absent_blank_node_text() {
    assert_eq!(lexical(&iri("urn:", "value").into()), Some("urn:value"));
    assert_eq!(lexical(&Literal::new_simple_literal("é").into()), Some("é"));
    assert_eq!(lexical(&BlankNode::default().into()), None);
}

#[test]
fn integer_datatypes_enforce_lexical_syntax_and_all_numeric_bounds() {
    for (kind, minimum, maximum) in [
        ("long", "-9223372036854775808", "9223372036854775807"),
        ("int", "-2147483648", "2147483647"),
        ("short", "-32768", "32767"),
        ("byte", "-128", "127"),
        ("unsignedLong", "0", "18446744073709551615"),
        ("unsignedInt", "0", "4294967295"),
        ("unsignedShort", "0", "65535"),
        ("unsignedByte", "0", "255"),
    ] {
        let dt = iri(XSD, kind);
        for text in [minimum.to_owned(), maximum.to_owned(), "+0001".to_owned()] {
            assert!(
                datatype(&Literal::new_typed_literal(&text, dt.clone()).into(), &dt),
                "{kind}: {text}"
            );
        }
        for bound in [
            minimum.parse::<BigInt>().unwrap() - BigInt::from(1),
            maximum.parse::<BigInt>().unwrap() + BigInt::from(1),
        ] {
            assert!(!datatype(
                &Literal::new_typed_literal(bound.to_string(), dt.clone()).into(),
                &dt
            ));
        }
        for invalid in ["", "+", "-", "1.0", " 1", "1 ", "١", "1e2", "--1"] {
            assert!(!datatype(
                &Literal::new_typed_literal(invalid, dt.clone()).into(),
                &dt
            ));
        }
    }
    for (kind, valid, invalid) in [
        ("integer", "-999999999999999999999999999999999", "1.2"),
        ("nonNegativeInteger", "0", "-1"),
        ("positiveInteger", "1", "0"),
        ("nonPositiveInteger", "0", "1"),
        ("negativeInteger", "-1", "0"),
    ] {
        let dt = iri(XSD, kind);
        assert!(datatype(
            &Literal::new_typed_literal(valid, dt.clone()).into(),
            &dt
        ));
        assert!(!datatype(
            &Literal::new_typed_literal(invalid, dt.clone()).into(),
            &dt
        ));
    }
}

#[test]
fn lexical_datatype_validation_covers_calendar_binary_and_string_contracts() {
    for (kind, valid, invalid) in [
        ("boolean", "false", "TRUE"),
        ("decimal", "+.01", "1.2.3"),
        ("float", "INF", "bad"),
        ("double", "NaN", "bad"),
        ("date", "2024-02-29", "2023-02-29"),
        ("time", "12:34:56Z", "25:00:00"),
        ("dateTime", "2024-02-29T12:34:56", "bad"),
        (
            "dateTimeStamp",
            "2024-02-29T12:34:56Z",
            "2024-02-29T12:34:56",
        ),
        ("duration", "P1Y2DT3H", "bad"),
        ("dayTimeDuration", "P2DT3H", "P1Y"),
        ("yearMonthDuration", "P1Y2M", "P1D"),
        ("gYear", "2024", "year"),
        ("gMonth", "--02", "--13"),
        ("gDay", "---29", "---32"),
        ("gYearMonth", "2024-02", "2024-13"),
        ("gMonthDay", "--02-29", "--02-30"),
        ("hexBinary", "Aa09", "ag"),
        ("normalizedString", "a b", "a\tb"),
        ("token", "a b", " a b"),
    ] {
        let dt = iri(XSD, kind);
        assert!(
            datatype(&Literal::new_typed_literal(valid, dt.clone()).into(), &dt),
            "{kind}: {valid}"
        );
        assert!(
            !datatype(&Literal::new_typed_literal(invalid, dt.clone()).into(), &dt),
            "{kind}: {invalid}"
        );
    }
    for invalid in ["", ".", "-.", "1e2", "1x"] {
        assert!(!datatype(
            &Literal::new_typed_literal(invalid, iri(XSD, "decimal")).into(),
            &iri(XSD, "decimal")
        ));
    }
    for invalid in ["a  b", "a\nb", "a\rb", "a\tb", "a b "] {
        assert!(!datatype(
            &Literal::new_typed_literal(invalid, iri(XSD, "token")).into(),
            &iri(XSD, "token")
        ));
    }
    let text: Term = Literal::new_simple_literal("text").into();
    assert!(datatype(&text, &iri(RDFS, "Literal")));
    assert!(datatype(&text, &iri(RDFS, "Datatype")));
    assert!(!datatype(&text, &iri(XSD, "integer")));
    assert!(!datatype(
        &iri("urn:", "node").into(),
        &iri(RDFS, "Literal")
    ));
    for dt in [
        iri(XSD, "anyURI"),
        iri(XSD, "unknown"),
        iri("urn:", "custom"),
    ] {
        assert!(datatype(
            &Literal::new_typed_literal("opaque", dt.clone()).into(),
            &dt
        ));
    }
}

#[test]
fn comparison_expression_distinguishes_false_unbound_and_invalid_result_shapes() {
    let data = Dataset::new();
    let value: Term = Literal::from(1).into();
    let other: Term = Literal::from(2).into();
    for (query, expected) in [
        (
            "SELECT ?value ?other (?value < ?other AS ?ok) WHERE {}",
            true,
        ),
        ("SELECT ?value ?other (false AS ?ok) WHERE {}", false),
        ("SELECT ?value ?other (1/0 AS ?ok) WHERE {}", false),
    ] {
        let query = SparqlEvaluator::new().parse_query(query).unwrap();
        assert_eq!(
            expression(&query, &data, &value, Some(&other)).unwrap(),
            expected
        );
    }
    for (query, message) in [
        ("ASK { VALUES ?value {1} }", "must return a solution"),
        (
            "SELECT ?value ?ok WHERE { FILTER(false) }",
            "returned no row",
        ),
    ] {
        let query = SparqlEvaluator::new().parse_query(query).unwrap();
        assert!(
            expression(&query, &data, &value, None)
                .unwrap_err()
                .to_string()
                .contains(message)
        );
    }
}
