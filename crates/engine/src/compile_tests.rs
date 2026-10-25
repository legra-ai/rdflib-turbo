use super::*;
use oxigraph::io::RdfFormat;

fn text(value: &str) -> Term {
    Literal::new_simple_literal(value).into()
}
fn node() -> Term {
    iri("urn:", "rule").into()
}

#[test]
fn rule_orders_reject_invalid_numeric_lexicals_and_non_numeric_terms() {
    assert_eq!(order(None, &node()).unwrap(), BigDecimal::from(0));
    for kind in ["integer", "decimal", "float", "double"] {
        assert_eq!(
            order(
                Some(Literal::new_typed_literal("1", iri(XSD, kind)).into()),
                &node()
            )
            .unwrap(),
            BigDecimal::from(1)
        );
        assert!(
            order(
                Some(Literal::new_typed_literal("invalid", iri(XSD, kind)).into()),
                &node()
            )
            .is_err()
        );
    }
    for value in [node(), text("1"), Literal::from(true).into()] {
        assert!(order(Some(value), &node()).is_err());
    }
}

#[test]
fn query_lowering_checks_literal_types_query_kinds_and_required_projection() {
    let data = Dataset::new();
    for (value, construct, message) in [
        (node(), false, "string literal"),
        (Literal::from(1).into(), false, "xsd:string"),
        (text("INVALID"), false, "parse error"),
        (text("ASK {}"), false, "must be SELECT"),
        (text("SELECT ?this WHERE {}"), true, "must be CONSTRUCT"),
        (text("SELECT ?missing WHERE {}"), false, "project ?this"),
    ] {
        assert!(
            query(&data, model::SparqlQuery::new(value), construct, &[])
                .err()
                .unwrap()
                .to_string()
                .contains(message)
        );
    }
    for (value, construct) in [
        ("SELECT ?this WHERE {}", false),
        ("CONSTRUCT { $this <urn:p> true } WHERE {}", true),
    ] {
        assert!(query(&data, model::SparqlQuery::new(text(value)), construct, &[]).is_ok());
    }
}

fn triple() -> model::RuleKind<Term> {
    model::RuleKind::Triple(Box::new(model::TripleRule {
        subject: model::NodeExpression::new(iri(SH, "this").into(), model::ExpressionKind::This),
        predicate: model::NodeExpression::new(
            iri("urn:", "p").into(),
            model::ExpressionKind::Constant,
        ),
        object: model::NodeExpression::new(text("value"), model::ExpressionKind::Constant),
    }))
}

#[test]
fn lowering_rejects_ambiguous_rule_bodies_and_unresolved_conditions() {
    let mut rule = model::ShaclRule::new(node(), triple());
    rule.kinds.push(triple());
    assert!(
        lower_rule(&Dataset::new(), rule, &HashMap::new(), &[])
            .err()
            .unwrap()
            .to_string()
            .contains("Multiple TripleRule")
    );
    let mut rule = model::ShaclRule::new(node(), triple());
    rule.conditions.push(iri("urn:", "unknown").into());
    assert!(
        lower_rule(&Dataset::new(), rule.clone(), &HashMap::new(), &[])
            .err()
            .unwrap()
            .to_string()
            .contains("Unknown condition")
    );
    let lowered = lower_rule(
        &Dataset::new(),
        rule,
        &HashMap::from([(iri("urn:", "unknown").into(), 3)]),
        &[],
    )
    .unwrap();
    assert_eq!(lowered.conditions, vec![3]);
    assert!(lowered.triple.is_some());
}

#[test]
fn custom_targets_require_exactly_one_value_for_each_mandatory_parameter() {
    let data = crate::IndexedShapesSource::parse_graph("@prefix sh: <http://www.w3.org/ns/shacl#>. <urn:type> sh:select 'SELECT ?this WHERE {}'; sh:parameter [sh:path <https://example.org/value>].", RdfFormat::Turtle, None).unwrap();
    let path = NamedNode::new("https://example.org/value").unwrap();
    for values in [None, Some(vec![]), Some(vec![text("a"), text("b")])] {
        let target = model::CustomTarget {
            id: node(),
            target_type: iri("urn:", "type").into(),
            parameters: values
                .map(|values| {
                    vec![model::ParameterValues {
                        path: path.clone(),
                        values,
                    }]
                })
                .unwrap_or_default(),
        };
        assert!(lower_target(&data, model::Target::Custom(target), &[]).is_err());
    }
}

#[test]
fn equal_order_rules_are_sorted_by_node_identity() {
    let data = crate::IndexedShapesSource::parse_graph("@prefix sh: <http://www.w3.org/ns/shacl#>. <urn:S> sh:targetNode <urn:s>; sh:rule <urn:z>, <urn:a>. <urn:z> a sh:TripleRule; sh:subject sh:this; sh:predicate <urn:p>; sh:object true. <urn:a> a sh:TripleRule; sh:subject sh:this; sh:predicate <urn:p>; sh:object false.", RdfFormat::Turtle, None).unwrap();
    let compiled = CompiledShapes::compile(data, true).unwrap();
    let shape = compiled
        .shapes
        .iter()
        .find(|s| s.node == iri("urn:", "S").into())
        .unwrap();
    assert_eq!(
        shape
            .rules
            .iter()
            .map(|r| r.node.clone())
            .collect::<Vec<_>>(),
        vec![iri("urn:", "a").into(), iri("urn:", "z").into()]
    );
}

#[test]
fn executable_resources_reject_unsupported_features() {
    for (body, advanced) in [
        ("<urn:f> a sh:Function.", true),
        ("<urn:f> a sh:SHACLFunction.", true),
        ("<urn:f> a sh:JSFunction.", true),
        ("<urn:S> sh:targetNode <urn:s>; a sh:UnknownShape.", true),
        ("<urn:S> sh:targetNode <urn:s>; sh:unknown true.", true),
    ] {
        let data = crate::IndexedShapesSource::parse_graph(
            &format!("@prefix sh: <http://www.w3.org/ns/shacl#>. {body}"),
            RdfFormat::Turtle,
            None,
        )
        .unwrap();
        assert!(
            matches!(
                CompiledShapes::compile(data, advanced),
                Err(Error::Unsupported(_))
            ),
            "{body}"
        );
    }
}

#[test]
fn named_shapes_compile_their_union_and_retain_original_graph_placement() {
    let source = crate::IndexedShapesSource::parse(
        "<urn:S> <http://www.w3.org/ns/shacl#targetNode> <urn:s> <urn:graph> .",
        RdfFormat::NQuads,
        None,
        None,
    )
    .unwrap();
    let compiled = CompiledShapes::compile(source, true).unwrap();
    assert_eq!(
        compiled.shape_nodes().collect::<Vec<_>>(),
        [&Term::from(iri("urn:", "S"))]
    );
    assert!(
        compiled
            .graph
            .contains_named_graph(&iri("urn:", "graph").into())
            .unwrap()
    );
}
