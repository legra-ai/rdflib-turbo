//! Preserve original RDFLib lazy-join barriers before BGP lowering loses node identity.
use crate::algebra::{GraphPattern, JoinAlgorithm};
use spargebra::{Query, SparqlParser};

fn parsed(text: &str) -> spargebra::algebra::GraphPattern {
    let Query::Select { pattern, .. } = SparqlParser::new().parse_query(text).unwrap() else {
        panic!("SELECT required");
    };
    pattern
}

fn top_join(pattern: &GraphPattern) -> &JoinAlgorithm {
    match pattern {
        GraphPattern::Project { inner, .. } => top_join(inner),
        GraphPattern::Join { algorithm, .. } => algorithm,
        _ => panic!("expected an original parsed Join"),
    }
}

#[test]
fn plain_subquery_is_lazy_but_distinct_and_slice_are_barriers() {
    for (suffix, lazy) in [("", true), (" LIMIT 1", false)] {
        let original = parsed(&format!(
            "SELECT ?s WHERE {{ VALUES ?s {{<urn:a>}} {{ SELECT ?s WHERE {{BIND(<urn:b> AS ?s)}}{suffix} }} }}"
        ));
        let lowered = GraphPattern::from_rdflib_algebra(&original);
        assert_eq!(
            matches!(top_join(&lowered), JoinAlgorithm::RdfLibLazy),
            lazy
        );
        assert!(!matches!(
            top_join(&GraphPattern::from(&original)),
            JoinAlgorithm::RdfLibLazy
        ));
    }
    let original = parsed(
        "SELECT ?s WHERE { VALUES ?s {<urn:a>} { SELECT DISTINCT ?s WHERE {BIND(<urn:b> AS ?s)} } }",
    );
    assert!(!matches!(
        top_join(&GraphPattern::from_rdflib_algebra(&original)),
        JoinAlgorithm::RdfLibLazy
    ));
}

#[test]
fn lowering_distinguishes_bgp_internal_joins_from_original_join_barriers() {
    let original = parsed(
        "SELECT ?s WHERE { ?s <urn:p> ?x . ?s <urn:q> ?y . { SELECT ?s WHERE {BIND(<urn:b> AS ?s)} } }",
    );
    let lowered = GraphPattern::from_rdflib_algebra(&original);
    assert!(matches!(top_join(&lowered), JoinAlgorithm::RdfLibLazy));
    let GraphPattern::Project { inner, .. } = lowered else {
        panic!("projection")
    };
    let GraphPattern::Join { left, .. } = *inner else {
        panic!("outer join")
    };
    assert!(matches!(
        *left,
        GraphPattern::Join {
            algorithm: JoinAlgorithm::HashBuildLeftProbeRight { .. },
            ..
        }
    ));
    let nested_join = parsed(
        "SELECT ?s WHERE { VALUES ?s {<urn:a>} { VALUES ?x {1} {SELECT ?s WHERE {BIND(<urn:b> AS ?s)}} } }",
    );
    assert!(!matches!(
        top_join(&GraphPattern::from_rdflib_algebra(&nested_join)),
        JoinAlgorithm::RdfLibLazy
    ));
}

#[test]
fn exists_and_expression_children_keep_original_join_metadata() {
    let original = parsed(
        "SELECT ?s WHERE { VALUES ?s {<urn:a>} { BIND(EXISTS { VALUES ?x {1} {SELECT ?x WHERE {BIND(2 AS ?x)}} } AS ?ok) } }",
    );
    let lowered = GraphPattern::from_rdflib_algebra(&original);
    assert!(!matches!(top_join(&lowered), JoinAlgorithm::RdfLibLazy));
    let GraphPattern::Project { inner, .. } = lowered else {
        panic!("projection")
    };
    let GraphPattern::Join { right, .. } = *inner else {
        panic!("join")
    };
    let GraphPattern::Extend {
        expression: crate::algebra::Expression::Exists(inner),
        ..
    } = *right
    else {
        panic!("exists")
    };
    assert!(matches!(
        *inner,
        GraphPattern::Join {
            algorithm: JoinAlgorithm::RdfLibLazy,
            ..
        }
    ));
}

#[test]
fn optional_retry_retains_hidden_source_variables_but_excludes_bind_expression_only_names() {
    let original = parsed(
        "SELECT ?x WHERE { {SELECT ?x WHERE { ?hidden <urn:p> ?x } } BIND(?expr AS ?alias) OPTIONAL {?x <urn:q> ?z} }",
    );
    let GraphPattern::Project { inner, .. } = GraphPattern::from_rdflib_algebra(&original) else {
        panic!("projection")
    };
    let GraphPattern::LeftJoin {
        algorithm: crate::algebra::LeftJoinAlgorithm::RdfLib { variables },
        ..
    } = *inner
    else {
        panic!("RDFLib optional metadata")
    };
    assert_eq!(
        variables.iter().map(|v| v.as_str()).collect::<Vec<_>>(),
        ["alias", "hidden", "x"]
    );
    let original =
        parsed("SELECT ?s WHERE { ?s <urn:p> ?x FILTER(?rel = ?x) OPTIONAL {?s <urn:q> ?z} }");
    let GraphPattern::Project { inner, .. } = GraphPattern::from_rdflib_algebra(&original) else {
        panic!("projection")
    };
    let GraphPattern::Filter { inner, .. } = *inner else {
        panic!("filter")
    };
    let GraphPattern::LeftJoin {
        algorithm: crate::algebra::LeftJoinAlgorithm::RdfLib { variables },
        ..
    } = *inner
    else {
        panic!("RDFLib optional metadata")
    };
    assert_eq!(
        variables.iter().map(|v| v.as_str()).collect::<Vec<_>>(),
        ["s", "x"]
    );
}
