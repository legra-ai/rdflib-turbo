//! RDFLib's translated EXISTS graph retains otherwise removable group joins.
use crate::algebra::{Expression, GraphPattern};
use crate::{Query, SparqlParser};

fn exists_body(text: &str, enabled: bool) -> GraphPattern {
    let parser = SparqlParser::new();
    let parser = if enabled {
        parser.with_rdflib_query_context()
    } else {
        parser
    };
    let Query::Select {
        pattern: GraphPattern::Project { inner, .. },
        ..
    } = parser.parse_query(text).unwrap()
    else {
        panic!("SELECT projection")
    };
    let GraphPattern::Filter {
        expr: Expression::Exists(inner),
        ..
    } = *inner
    else {
        panic!("EXISTS filter")
    };
    *inner
}

#[test]
fn explicit_context_distinguishes_direct_grouped_and_subselect_exists() {
    let direct = "SELECT ?s WHERE {FILTER EXISTS {BIND(<urn:b> AS ?s)}}";
    let grouped = "SELECT ?s WHERE {FILTER EXISTS {{BIND(<urn:b> AS ?s)}}}";
    let subselect = "SELECT ?s WHERE {FILTER EXISTS {{SELECT ?s WHERE {BIND(<urn:b> AS ?s)}}}}";
    assert!(matches!(
        exists_body(direct, true),
        GraphPattern::Extend { .. }
    ));
    for text in [grouped, subselect] {
        let GraphPattern::Join { left, right } = exists_body(text, true) else {
            panic!("unsimplified EXISTS group join")
        };
        assert!(matches!(*left, GraphPattern::Bgp { patterns } if patterns.is_empty()));
        assert!(matches!(
            *right,
            GraphPattern::Extend { .. } | GraphPattern::Project { .. }
        ));
    }
    assert!(matches!(
        exists_body(grouped, false),
        GraphPattern::Extend { .. }
    ));
    assert!(matches!(
        exists_body(subselect, false),
        GraphPattern::Project { .. }
    ));
}

#[test]
fn explicit_context_changes_neither_regular_groups_nor_sibling_scope() {
    for query in [
        "SELECT ?s WHERE {{BIND(<urn:b> AS ?s)}}",
        "SELECT ?s WHERE { {BIND(EXISTS {{BIND(<urn:b> AS ?s)}} AS ?found)} {BIND(1 AS ?x)} }",
    ] {
        let normal = SparqlParser::new().parse_query(query).unwrap();
        let contextual = SparqlParser::new()
            .with_rdflib_query_context()
            .parse_query(query)
            .unwrap();
        if !query.contains("EXISTS") {
            assert_eq!(normal, contextual);
        } else {
            let Query::Select {
                pattern: GraphPattern::Project { inner, .. },
                ..
            } = contextual
            else {
                panic!("projection")
            };
            let GraphPattern::Join { left, right } = *inner else {
                panic!("regular join")
            };
            assert!(matches!(*left, GraphPattern::Extend { .. }));
            assert!(matches!(*right, GraphPattern::Extend { .. }));
        }
    }
}

#[test]
fn prefixed_function_backtracking_cannot_enter_exists_scope() {
    let query = "PREFIX EXISTS: <urn:custom:> SELECT ?s WHERE {BIND(EXISTS:custom() AS ?found) {BIND(1 AS ?s)}}";
    assert_eq!(
        SparqlParser::new().parse_query(query).unwrap(),
        SparqlParser::new()
            .with_rdflib_query_context()
            .parse_query(query)
            .unwrap()
    );
}

#[test]
fn nested_not_exists_scope_does_not_change_following_regular_group() {
    let query = "SELECT ?s WHERE {BIND(EXISTS {FILTER NOT EXISTS {{BIND(<urn:b> AS ?s)}}} AS ?found) {BIND(1 AS ?s)}}";
    let Query::Select {
        pattern: GraphPattern::Project { inner, .. },
        ..
    } = SparqlParser::new()
        .with_rdflib_query_context()
        .parse_query(query)
        .unwrap()
    else {
        panic!("projection")
    };
    let GraphPattern::Join { left, right } = *inner else {
        panic!("normal group join")
    };
    assert!(matches!(*left, GraphPattern::Extend { .. }));
    assert!(matches!(*right, GraphPattern::Extend { .. }));
}
