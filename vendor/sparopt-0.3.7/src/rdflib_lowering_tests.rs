//! Preserve original RDFLib lazy-join barriers before BGP lowering loses node identity.
use crate::algebra::{GraphPattern, JoinAlgorithm};
use spargebra::{Query, SparqlParser};

#[test]
fn optional_retry_does_not_treat_values_row_keys_as_source_variables() {
    let original = parsed("SELECT ?x WHERE { VALUES ?x {1} OPTIONAL { ?s <urn:p> ?z } }");
    let GraphPattern::Project { inner, .. } = GraphPattern::from_rdflib_algebra(&original) else {
        panic!("projection")
    };
    let GraphPattern::LeftJoin {
        algorithm: crate::algebra::LeftJoinAlgorithm::RdfLib { variables },
        ..
    } = *inner
    else {
        panic!("OPTIONAL")
    };
    assert!(variables.is_empty());
}

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
    // Leaf optimization may select a correlated indexed join, but it must not
    // mistake the BGP-created connection for an original RDFLib lazy Join.
    assert!(!matches!(
        *left,
        GraphPattern::Join {
            algorithm: JoinAlgorithm::RdfLibLazy,
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
fn exists_raw_syntax_does_not_block_parent_join_but_its_translated_graph_is_independent() {
    let original = parsed(
        "SELECT ?s WHERE { VALUES ?s {<urn:a>} { BIND(EXISTS { VALUES ?x {1} {SELECT ?x WHERE {BIND(2 AS ?x)}} } AS ?ok) } }",
    );
    let lowered = GraphPattern::from_rdflib_algebra(&original);
    // Original RDFLib analyse visits raw EXISTS syntax, not its translated graph.
    assert!(matches!(top_join(&lowered), JoinAlgorithm::RdfLibLazy));
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
            algorithm: JoinAlgorithm::HashBuildLeftProbeRight { .. },
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

#[test]
fn exists_context_preserves_independent_wrapper_joins_and_optional_metadata() {
    let Query::Select { pattern, .. } = SparqlParser::new()
        .with_rdflib_query_context()
        .parse_query(
            "SELECT ?s WHERE {FILTER EXISTS {{BIND(<urn:b> AS ?s)} OPTIONAL {?s <urn:p> ?o}}}",
        )
        .unwrap()
    else {
        panic!("SELECT")
    };
    let GraphPattern::Project { inner, .. } = GraphPattern::from_rdflib_algebra(&pattern) else {
        panic!("projection")
    };
    let GraphPattern::Filter {
        expression: crate::algebra::Expression::Exists(inner),
        ..
    } = *inner
    else {
        panic!("exists")
    };
    let GraphPattern::LeftJoin {
        left,
        algorithm: crate::algebra::LeftJoinAlgorithm::RdfLib { variables },
        ..
    } = *inner
    else {
        panic!("optional")
    };
    assert_eq!(
        variables.iter().map(|v| v.as_str()).collect::<Vec<_>>(),
        ["s"]
    );
    assert!(matches!(
        *left,
        GraphPattern::Join {
            algorithm: JoinAlgorithm::HashBuildLeftProbeRight { .. },
            ..
        }
    ));
}

#[test]
fn ordered_lowering_optimizes_only_bgp_leaves_and_can_disable_even_that() {
    let original = parsed(
        "SELECT ?s WHERE {?s <urn:p> ?x . ?s <urn:q> ?y . {SELECT ?s WHERE {BIND(<urn:b> AS ?s)}}}",
    );
    let extract = |pattern: GraphPattern| {
        let GraphPattern::Project { inner, .. } = pattern else {
            panic!("projection")
        };
        let GraphPattern::Join {
            left,
            algorithm: JoinAlgorithm::RdfLibLazy,
            ..
        } = *inner
        else {
            panic!("outer ordered join")
        };
        *left
    };
    let optimized = extract(GraphPattern::from_rdflib_algebra(&original));
    let connected = match optimized {
        GraphPattern::Join {
            algorithm: JoinAlgorithm::HashBuildLeftProbeRight { keys },
            ..
        } => keys.iter().any(|variable| variable.as_str() == "s"),
        #[cfg(feature = "sep-0006")]
        GraphPattern::Lateral { .. } => true,
        other => panic!("expected a connected indexed BGP plan: {other:?}"),
    };
    assert!(connected);
    assert!(
        matches!(extract(GraphPattern::from_rdflib_algebra_unoptimized(&original)), GraphPattern::Join { algorithm: JoinAlgorithm::HashBuildLeftProbeRight { keys }, .. } if keys.is_empty())
    );
}

#[test]
fn grouped_distinct_and_sliced_exists_do_not_turn_an_outer_lazy_join_into_a_hash_join() {
    // Pinned RDFLib 7.6.0 outer Join.lazy is true for all three source forms.
    // With unused init binding, their public result is (urn:a, true), because
    // the outer VALUES row is merged over the correlated right BIND result.
    for inner in [
        "{BIND(<urn:b> AS ?s)}",
        "{SELECT DISTINCT ?s WHERE {BIND(<urn:b> AS ?s)}}",
        "{SELECT ?s WHERE {BIND(<urn:b> AS ?s)} LIMIT 1}",
    ] {
        let query = format!(
            "SELECT ?s ?ok WHERE {{VALUES ?s {{<urn:a>}} {{BIND(<urn:b> AS ?s) BIND(EXISTS {{{inner}}} AS ?ok)}}}}"
        );
        let Query::Select { pattern, .. } = SparqlParser::new()
            .with_rdflib_query_context()
            .parse_query(&query)
            .unwrap()
        else {
            panic!("SELECT")
        };
        assert!(matches!(
            top_join(&GraphPattern::from_rdflib_algebra(&pattern)),
            JoinAlgorithm::RdfLibLazy
        ));
    }
}
