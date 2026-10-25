use super::*;
use crate::model::{Component as C, ShaclPath};

#[test]
fn lowerer_rejects_unresolved_shape_references_and_non_predicate_pairs() {
    let data = Dataset::new();
    let node: Term = iri("urn:", "node").into();
    let missing: Term = iri("urn:", "missing").into();
    for component in [
        C::Node(missing.clone()),
        C::Not(missing.clone()),
        C::And(vec![missing.clone()]),
        C::Or(vec![missing.clone()]),
        C::Xone(vec![missing.clone()]),
        C::QualifiedValueShape {
            shape: missing,
            min_count: None,
            max_count: None,
            disjoint: false,
        },
    ] {
        assert!(
            lower_one(
                &data,
                &node,
                true,
                &HashMap::new(),
                component,
                Default::default(),
                None,
            )
            .err()
            .unwrap()
            .to_string()
            .contains("Unknown shape")
        );
    }
    let path = ShaclPath::Predicate(NamedNode::new("urn:p").unwrap());
    assert!(
        lower_one(
            &data,
            &node,
            false,
            &HashMap::new(),
            C::LessThan(path.clone()),
            Default::default(),
            None,
        )
        .is_err()
    );
    assert!(
        lower_one(
            &data,
            &node,
            true,
            &HashMap::new(),
            C::Equals(ShaclPath::Inverse(Box::new(path))),
            Default::default(),
            None,
        )
        .is_err()
    );
    assert!(
        lower_one(
            &data,
            &node,
            true,
            &HashMap::new(),
            C::UniqueLang(false),
            Default::default(),
            None,
        )
        .unwrap()
        .is_none()
    );
    assert!(
        lower_one(
            &data,
            &node,
            true,
            &HashMap::new(),
            C::Closed {
                is_closed: false,
                ignored_properties: vec![]
            },
            Default::default(),
            None,
        )
        .unwrap()
        .is_none()
    );
    assert!(
        lower_one(
            &data,
            &node,
            true,
            &HashMap::new(),
            C::Pattern {
                pattern: Literal::new_simple_literal("a").into(),
                flags: Some(node.clone())
            },
            Default::default(),
            None,
        )
        .is_err()
    );
}

#[test]
fn qualified_boolean_counts_keep_python_integer_and_display_semantics() {
    for (value, display, count) in [
        ("false", "False", 0),
        ("0", "False", 0),
        ("true", "True", 1),
        ("1", "True", 1),
    ] {
        let parameter = integer(
            Literal::new_typed_literal(value, iri(XSD, "boolean")).into(),
            "qualifiedMinCount",
        )
        .unwrap();
        assert_eq!(parameter.to_string(), display);
        assert_eq!(parameter.number, BigInt::from(count));
        assert!(parameter.allows(count, true));
        assert!(parameter.allows(count, false));
    }
    assert!(
        integer(
            Literal::new_typed_literal("invalid", iri(XSD, "boolean")).into(),
            "qualifiedMaxCount"
        )
        .is_err()
    );
    assert!(integer(iri("urn:", "notLiteral").into(), "minCount").is_err());
}

#[test]
fn membership_empty_resources_are_empty_allowed_sets() {
    let shape = NamedNode::new("urn:membership:shape").unwrap();
    for head in [
        Term::from(NamedNode::new("urn:membership:absent").unwrap()),
        Term::from(BlankNode::new("absent").unwrap()),
        Term::from(Literal::new_simple_literal("absent")),
        Term::from(iri(RDF, "nil")),
    ] {
        let data = Dataset::from_iter([Quad::new(
            shape.clone(),
            iri(SH, "in"),
            head,
            GraphName::DefaultGraph,
        )]);
        let values = read(&data, &shape.clone().into(), true, Default::default()).unwrap();
        assert!(matches!(&values[..], [C::In(allowed)] if allowed.is_empty()));
    }
}

#[test]
fn membership_missing_rest_keeps_the_first_value() {
    let shape = NamedNode::new("urn:membership:shape").unwrap();
    let head = NamedNode::new("urn:membership:head").unwrap();
    let value: Term = NamedNode::new("urn:membership:value").unwrap().into();
    let data = Dataset::from_iter([
        Quad::new(
            shape.clone(),
            iri(SH, "in"),
            head.clone(),
            GraphName::DefaultGraph,
        ),
        Quad::new(
            head,
            iri(RDF, "first"),
            value.clone(),
            GraphName::DefaultGraph,
        ),
    ]);
    let values = read(&data, &shape.into(), true, Default::default()).unwrap();
    assert!(matches!(&values[..], [C::In(allowed)] if allowed == &[value]));
}

#[test]
fn membership_cycles_retain_the_original_error_text() {
    let shape = NamedNode::new("urn:membership:shape").unwrap();
    let head = NamedNode::new("urn:membership:head").unwrap();
    let data = Dataset::from_iter([
        Quad::new(
            shape.clone(),
            iri(SH, "in"),
            head.clone(),
            GraphName::DefaultGraph,
        ),
        Quad::new(
            head.clone(),
            iri(RDF, "rest"),
            head,
            GraphName::DefaultGraph,
        ),
    ]);
    assert_eq!(
        read(&data, &shape.into(), true, Default::default())
            .unwrap_err()
            .to_string(),
        "List contains a recursive rdf:rest reference"
    );
}

#[test]
fn host_unknown_digits_retain_component_specific_conversion_failures() {
    let digits = DecimalDigitPolicy::new(|_| None);
    for name in ["minCount", "maxCount"] {
        let term = Literal::new_typed_literal("\u{11bf0}", iri(XSD, "integer")).into();
        assert!(matches!(
            integer_with_policy(term, name, digits),
            Err(Error::TypeError(_))
        ));
    }
    for name in [
        "minLength",
        "maxLength",
        "qualifiedMinCount",
        "qualifiedMaxCount",
    ] {
        let term = Literal::new_typed_literal("\u{11bf0}", iri(XSD, "integer")).into();
        assert!(matches!(
            integer_with_policy(term, name, digits),
            Err(Error::ConstraintLoad { .. })
        ));
    }
}

#[test]
fn typed_single_line_declarations_preserve_enabled_and_disabled_states() {
    let data = Dataset::new();
    let node: Term = iri("urn:", "single-line").into();
    for value in [false, true] {
        let lowered = lower_one(
            &data,
            &node,
            false,
            &HashMap::new(),
            C::SingleLine(value),
            Default::default(),
            None,
        )
        .unwrap();
        if value {
            assert!(matches!(lowered, Some(Constraint::SingleLine)));
        } else {
            assert!(lowered.is_none());
        }
    }
}

#[test]
fn public_single_line_reader_preserves_booleans_and_rejects_invalid_parameters() {
    let shape = NamedNode::new("urn:single-line:shape").unwrap();
    for property in [false, true] {
        for value in [true, false] {
            let data = Dataset::from_iter([Quad::new(
                shape.clone(),
                iri(SH, "singleLine"),
                Literal::from(value),
                GraphName::DefaultGraph,
            )]);
            assert!(matches!(
                read(
                    &data,
                    &Term::from(shape.clone()),
                    property,
                    Default::default()
                )
                .unwrap().as_slice(),
                [C::SingleLine(enabled)] if *enabled == value
            ));
        }
        for value in [Literal::new_simple_literal("not boolean"), Literal::from(1)] {
            let data = Dataset::from_iter([Quad::new(
                shape.clone(),
                iri(SH, "singleLine"),
                value,
                GraphName::DefaultGraph,
            )]);
            assert!(
                read(
                    &data,
                    &Term::from(shape.clone()),
                    property,
                    Default::default()
                )
                .is_err()
            );
        }
    }
}
