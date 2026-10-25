use super::*;
use crate::model::{ExpressionKind as K, NodeExpression as E, ShaclPath};
use oxigraph::io::RdfFormat;

fn graph(body: &str) -> Dataset {
    crate::parse_graph(
        &format!(
            "@prefix sh: <http://www.w3.org/ns/shacl#>. @prefix ex: <https://example.org/>. {body}"
        ),
        RdfFormat::Turtle,
        None,
    )
    .unwrap()
}

fn named(name: &str) -> Term {
    iri("https://example.org/", name).into()
}
fn blank(name: &str) -> Term {
    BlankNode::new(name).unwrap().into()
}

#[test]
fn expressions_read_lower_and_evaluate_union_intersection_filter_path_and_functions() {
    let data = graph(
        r#"
        ex:identity a sh:SPARQLFunction; sh:parameter [sh:path ex:value];
            sh:select "SELECT (?value AS ?result) WHERE {}".
        ex:focus ex:p ex:a, ex:b.
        _:union sh:union (sh:this ex:a).
        _:intersection sh:intersection ([sh:path ex:p] [sh:union (ex:b ex:c)]).
        _:filter sh:filterShape ex:Check; sh:nodes [sh:path ex:p].
        _:call ex:identity ([sh:union (ex:a ex:b)]).
        _:path sh:path ex:p; sh:nodes ex:ignored.
    "#,
    );
    let functions = Functions::compile(&data, &[]).unwrap();
    assert!(functions.contains(&iri("https://example.org/", "identity")));
    let shapes = HashMap::from([(named("Check"), 7)]);
    for (name, expected) in [
        ("union", HashSet::from([named("focus"), named("a")])),
        ("intersection", HashSet::from([named("b")])),
        ("filter", HashSet::from([named("a")])),
        ("call", HashSet::from([named("a"), named("b")])),
        ("path", HashSet::from([named("a"), named("b")])),
    ] {
        let expression = NodeExpression::lower(
            NodeExpression::read(&data, &blank(name), &functions).unwrap(),
            &shapes,
        )
        .unwrap();
        let actual = expression
            .evaluate(&named("focus"), &data, &functions, &mut |index, value| {
                assert_eq!(index, 7);
                Ok(value == &named("a"))
            })
            .unwrap();
        assert_eq!(actual, expected, "{name}");
    }
}

#[test]
fn expression_reader_rejects_cycles_missing_filter_inputs_and_unknown_calls() {
    let functions = Functions::default();
    for (body, message) in [
        ("_:e sh:union (_:e).", "Cyclic SHACL node expression"),
        ("_:e sh:intersection (_:e).", "Cyclic SHACL node expression"),
        ("_:e sh:filterShape ex:Check.", "requires sh:nodes"),
        ("_:e ex:unknown ().", "exactly one known function"),
    ] {
        assert!(
            NodeExpression::read(&graph(body), &blank("e"), &functions)
                .err()
                .unwrap()
                .to_string()
                .contains(message)
        );
    }
}

#[test]
fn lowering_rejects_unknown_filter_shapes_and_unsupported_nested_path_inputs() {
    let constant = || E::new(named("a"), K::Constant);
    let filter = E::new(
        blank("filter"),
        K::Filter {
            shape: named("missing"),
            nodes: Box::new(constant()),
        },
    );
    assert!(
        NodeExpression::lower(filter, &HashMap::new())
            .err()
            .unwrap()
            .to_string()
            .contains("Unknown filter shape")
    );
    let path = E::new(
        blank("path"),
        K::Path {
            path: ShaclPath::Predicate(NamedNode::new("https://example.org/p").unwrap()),
            path_node: None,
            nodes: Some(Box::new(constant())),
        },
    );
    assert!(matches!(
        NodeExpression::lower(path, &HashMap::new()),
        Err(Error::Unsupported(_))
    ));
}

#[test]
fn empty_sets_and_filter_failures_do_not_invent_values() {
    let data = Dataset::new();
    let functions = Functions::default();
    for expression in [
        NodeExpression::Union(vec![]),
        NodeExpression::Intersection(vec![]),
    ] {
        assert!(
            expression
                .evaluate(&named("focus"), &data, &functions, &mut |_, _| Ok(true))
                .unwrap()
                .is_empty()
        );
    }
    let filter = NodeExpression::Filter {
        shape: 0,
        nodes: Box::new(NodeExpression::This),
    };
    assert!(
        filter
            .evaluate(&named("focus"), &data, &functions, &mut |_, _| Ok(false))
            .unwrap()
            .is_empty()
    );
    assert!(
        filter
            .evaluate(&named("focus"), &data, &functions, &mut |_, _| Err(
                Error::Invalid("filter failed".into())
            ))
            .unwrap_err()
            .to_string()
            .contains("filter failed")
    );
}
