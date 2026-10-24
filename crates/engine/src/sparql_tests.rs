use super::*;
use oxigraph::io::RdfFormat;

#[test]
fn prefix_resolution_validates_literal_kinds_iris_conflicts_and_import_cycles() {
    let node: Term = iri("urn:", "prefixes").into();
    for declaration in [
        "sh:prefix <urn:notLiteral>; sh:namespace 'urn:ex:'",
        "sh:prefix 'ex'@en; sh:namespace 'urn:ex:'",
        "sh:prefix 'ex'; sh:namespace 1",
        "sh:prefix 'ex'; sh:namespace 'invalid IRI'",
        "sh:prefix 'ex'",
    ] {
        let data = parse(&format!("@prefix sh: <http://www.w3.org/ns/shacl#>. <urn:prefixes> sh:declare [{declaration}]."), RdfFormat::Turtle, None).unwrap();
        assert!(prefixes(&data, vec![node.clone()], &[]).is_err());
    }
    assert!(
        prefixes(
            &Dataset::new(),
            vec![],
            &[("bad".into(), "invalid IRI".into())]
        )
        .is_err()
    );
    let data = parse("@prefix sh: <http://www.w3.org/ns/shacl#>. <urn:prefixes> <http://www.w3.org/2002/07/owl#imports> <urn:prefixes>; sh:declare [sh:prefix 'ex'; sh:namespace 'urn:a:'], [sh:prefix 'ex'; sh:namespace 'urn:b:'].", RdfFormat::Turtle, None).unwrap();
    assert!(
        prefixes(&data, vec![node], &[])
            .unwrap_err()
            .to_string()
            .contains("Conflicting")
    );
}

#[test]
fn expression_visitor_finds_hidden_shapes_graph_references_in_every_operator() {
    for expression in [
        "?shapesGraph",
        "BOUND(?shapesGraph)",
        "?this || ?shapesGraph",
        "?this && ?shapesGraph",
        "?this = ?shapesGraph",
        "sameTerm(?this, ?shapesGraph)",
        "?this > ?shapesGraph",
        "?this >= ?shapesGraph",
        "?this < ?shapesGraph",
        "?this <= ?shapesGraph",
        "?this + ?shapesGraph",
        "?this - ?shapesGraph",
        "?this * ?shapesGraph",
        "?this / ?shapesGraph",
        "?this IN (?shapesGraph)",
        "+?shapesGraph",
        "-?shapesGraph",
        "!?shapesGraph",
        "IF(?this, ?this, ?shapesGraph)",
        "COALESCE(?this, ?shapesGraph)",
        "STR(?shapesGraph)",
        "EXISTS { GRAPH ?shapesGraph {?s ?p ?o} }",
    ] {
        let mut query = SparqlParser::new()
            .parse_query(&format!(
                "SELECT ?this ({expression} AS ?result) WHERE {{}}"
            ))
            .unwrap();
        let Query::Select { pattern, .. } = &mut query else {
            panic!("SELECT expected")
        };
        let mut visitor = ConstraintQuery {
            path: None,
            shapes_graph: false,
            prebound: &[],
        };
        visitor.pattern(pattern).unwrap();
        assert!(visitor.shapes_graph, "{expression}");
    }
}

#[test]
fn visitor_rejects_path_outside_predicates_and_disallowed_graph_operations() {
    let prebound = [Variable::new_unchecked("this")];
    let mut visitor = ConstraintQuery {
        path: None,
        shapes_graph: false,
        prebound: &prebound,
    };
    assert!(visitor.assignment(&prebound[0]).is_err());
    assert!(visitor.variable(&Variable::new_unchecked("PATH")).is_err());
    let empty = || Box::new(GraphPattern::Bgp { patterns: vec![] });
    assert!(
        visitor
            .pattern(&mut GraphPattern::Lateral {
                left: empty(),
                right: empty()
            })
            .is_err()
    );
    for query in [
        "SELECT ?this WHERE { ?this ?p ?o MINUS { ?this ?q ?v } }",
        "SELECT ?this WHERE { SERVICE <urn:service> { ?this ?p ?o } }",
        "SELECT ?this WHERE { VALUES ?this { <urn:s> } }",
        "SELECT ?this WHERE { ?this $PATH ?value }",
        "SELECT ?missing WHERE {}",
    ] {
        let Query::Select { mut pattern, .. } = SparqlParser::new().parse_query(query).unwrap()
        else {
            panic!("SELECT expected")
        };
        assert!(visitor.pattern(&mut pattern).is_err(), "{query}");
    }
}

#[test]
fn property_path_algebra_preserves_all_supported_operators() {
    let p = || Path::Predicate(iri("urn:", "p"));
    for path in [
        Path::Inverse(Box::new(p())),
        Path::ZeroOrMore(Box::new(p())),
        Path::OneOrMore(Box::new(p())),
        Path::ZeroOrOne(Box::new(p())),
        Path::Sequence(vec![p(), p()]),
        Path::Alternative(vec![p(), p()]),
    ] {
        let mut parsed = SparqlParser::new()
            .parse_query("SELECT ?this WHERE { ?this $PATH ?value }")
            .unwrap();
        let Query::Select { pattern, .. } = &mut parsed else {
            panic!("SELECT expected")
        };
        let mut visitor = ConstraintQuery {
            path: Some(&path),
            shapes_graph: false,
            prebound: &[],
        };
        visitor.pattern(pattern).unwrap();
        assert!(!parsed.to_string().contains("?PATH"));
    }
}

#[test]
fn visitor_preserves_valid_nested_graph_operators_and_aggregates() {
    for (text, uses_shapes) in [
        (
            "SELECT DISTINCT ?this ?value WHERE {?this <urn:p> ?value OPTIONAL {?this <urn:q> ?other FILTER(?value != ?other)}} ORDER BY ASC(?value) DESC(?this) LIMIT 5 OFFSET 1",
            false,
        ),
        (
            "SELECT REDUCED ?this WHERE {{?this <urn:p> ?v} UNION {GRAPH ?shapesGraph {?this <urn:q> ?v}}}",
            true,
        ),
        (
            "SELECT ?this (COUNT(*) AS ?count) (SUM(?value) AS ?total) WHERE {?this <urn:p> ?value} GROUP BY ?this",
            false,
        ),
        ("SELECT ?this WHERE {?this (<urn:p>/<urn:q>) ?value}", false),
    ] {
        let mut query = SparqlParser::new().parse_query(text).unwrap();
        let Query::Select { pattern, .. } = &mut query else {
            panic!("SELECT expected")
        };
        let mut visitor = ConstraintQuery {
            path: None,
            shapes_graph: false,
            prebound: &[],
        };
        visitor.pattern(pattern).unwrap();
        assert_eq!(visitor.shapes_graph, uses_shapes, "{text}");
        assert!(SparqlParser::new().parse_query(&query.to_string()).is_ok());
    }
    let data = parse("@prefix sh: <http://www.w3.org/ns/shacl#>. <urn:prefixes> <http://www.w3.org/2002/07/owl#imports> <urn:prefixes>; sh:declare [sh:prefix 'ex'; sh:namespace 'urn:a:'].", RdfFormat::Turtle, None).unwrap();
    assert_eq!(
        prefixes(&data, vec![iri("urn:", "prefixes").into()], &[]).unwrap(),
        "PREFIX ex: <urn:a:>\n"
    );
}

#[test]
fn source_scope_projection_visits_custom_aggregate_and_preserves_metadata() {
    let mut query = SparqlParser::new()
        .with_rdflib_query_context()
        .with_custom_aggregate_function(NamedNode::new("urn:aggregate").unwrap())
        .parse_query("SELECT ?this WHERE {FILTER EXISTS {{SELECT ?this (<urn:aggregate>(?shapesGraph) AS ?n) WHERE {?this ?p ?o} GROUP BY ?this}}}")
        .unwrap();
    let Query::Select { pattern, .. } = &mut query else {
        panic!("SELECT")
    };
    let prebound = [Variable::new_unchecked("currentShape")];
    let mut visitor = ConstraintQuery {
        path: None,
        shapes_graph: false,
        prebound: &prebound,
    };
    visitor.pattern(pattern).unwrap();
    assert!(
        visitor.shapes_graph,
        "custom aggregate argument was skipped"
    );
    let GraphPattern::Project { inner, .. } = pattern else {
        panic!("outer project")
    };
    let GraphPattern::Filter {
        expr: Expression::Exists(inner),
        ..
    } = inner.as_ref()
    else {
        panic!("EXISTS")
    };
    let GraphPattern::Join { right, .. } = inner.as_ref() else {
        panic!("source group")
    };
    let GraphPattern::ProjectWithSourceScope {
        variables,
        source_variables,
        ..
    } = right.as_ref()
    else {
        panic!("source projection")
    };
    assert!(variables.contains(&prebound[0]));
    assert!(!source_variables.contains(&prebound[0]));
    assert_eq!(source_variables.len(), 2);
}
