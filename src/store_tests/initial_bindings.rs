//! Original-RDFLib query contexts preserve native ownership and algebra scopes.

use oxigraph::model::{GraphName, Literal, Quad, Term, Variable};
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;

use super::node;

#[test]
fn rdflib_exists_optional_does_not_retry_an_unanalyzed_left_scope() {
    let store = Store::new().unwrap();
    store
        .insert(
            Quad::new(
                node("a"),
                node("p"),
                Literal::from("A"),
                GraphName::DefaultGraph,
            )
            .as_ref(),
        )
        .unwrap();
    store
        .insert(
            Quad::new(
                node("b"),
                node("p"),
                Literal::from("B"),
                GraphName::DefaultGraph,
            )
            .as_ref(),
        )
        .unwrap();
    let QueryResults::Boolean(actual) = SparqlEvaluator::new()
        .with_rdflib_query_context()
        .parse_query("ASK { VALUES ?s { <urn:a> } FILTER EXISTS { BIND(1 AS ?x) OPTIONAL { ?s <urn:p> \"B\" } } }")
        .unwrap()
        .with_initial_bindings([(Variable::new("unused").unwrap(), node("a").into())])
        .on_store(&store).execute().unwrap()
    else { panic!("Expected ASK") };
    assert!(actual);
}

#[test]
fn rdflib_initial_bindings_cross_projections_and_allow_bind_replacement() {
    let store = Store::new().unwrap();
    store
        .insert(
            Quad::new(
                node("a"),
                node("p"),
                Literal::from("A"),
                GraphName::DefaultGraph,
            )
            .as_ref(),
        )
        .unwrap();
    store
        .insert(
            Quad::new(
                node("b"),
                node("p"),
                Literal::from("B"),
                GraphName::DefaultGraph,
            )
            .as_ref(),
        )
        .unwrap();
    // bounded: reference-derived projection and correlated merge contracts.
    for (query, expected) in [
        (
            "SELECT ?o WHERE { ?s <urn:p> ?o }",
            Some(Literal::from("A").into()),
        ),
        (
            "SELECT ?o WHERE { { SELECT ?o WHERE { ?s <urn:p> ?o } } }",
            Some(Literal::from("A").into()),
        ),
        ("SELECT (?s AS ?alias) WHERE {}", Some(node("a").into())),
        ("SELECT ?s WHERE { VALUES ?s { <urn:b> } }", None),
        (
            "SELECT ?s WHERE { BIND(<urn:b> AS ?s) }",
            Some(node("b").into()),
        ),
        (
            "SELECT ?s WHERE { { SELECT ?s WHERE { BIND(<urn:b> AS ?s) } } }",
            Some(node("b").into()),
        ),
        (
            "SELECT ?s WHERE { VALUES ?s { <urn:a> } { SELECT ?s WHERE { BIND(<urn:b> AS ?s) } } }",
            Some(node("a").into()),
        ),
        (
            "SELECT ?s WHERE { BIND(<urn:a> AS ?s) { SELECT ?s WHERE { BIND(<urn:b> AS ?s) } } }",
            Some(node("a").into()),
        ),
        (
            "SELECT ?s WHERE { VALUES ?s { <urn:a> } { SELECT ?s WHERE { VALUES ?s { <urn:b> } } } }",
            None,
        ),
    ] {
        let QueryResults::Solutions(solutions) = SparqlEvaluator::new()
            .parse_query(query)
            .unwrap()
            .with_initial_bindings([(Variable::new("s").unwrap(), node("a").into())])
            .on_store(&store)
            .execute()
            .unwrap()
        else {
            panic!("Expected SELECT")
        };
        assert_eq!(solutions.variables().len(), 1, "{query}");
        let values = solutions
            .map(|row| row.unwrap().get(0).unwrap().clone())
            .collect::<Vec<Term>>();
        assert_eq!(values, expected.into_iter().collect::<Vec<_>>(), "{query}");
    }
}

#[test]
fn rdflib_initial_bindings_do_not_expand_strict_substitution_scope() {
    let store = Store::new().unwrap();
    let result = SparqlEvaluator::new()
        .parse_query("SELECT ?o WHERE { ?s <urn:p> ?o }")
        .unwrap()
        .with_initial_bindings([(Variable::new("unused").unwrap(), node("a").into())])
        .substitute_variable(Variable::new("s").unwrap(), node("a"))
        .on_store(&store)
        .execute();
    assert!(
        result.is_err(),
        "strict substitutions cannot bind hidden variables"
    );
}

#[test]
fn empty_rdflib_initial_context_still_uses_correlated_join_semantics() {
    let store = Store::new().unwrap();
    let QueryResults::Solutions(mut solutions) = SparqlEvaluator::new()
        .parse_query(
            "SELECT ?s WHERE { VALUES ?s { <urn:a> } { SELECT ?s WHERE { BIND(<urn:b> AS ?s) } } }",
        )
        .unwrap()
        .with_initial_bindings([])
        .on_store(&store)
        .execute()
        .unwrap()
    else {
        panic!("Expected SELECT")
    };
    assert_eq!(
        solutions.next().unwrap().unwrap().get(0),
        Some(&node("a").into())
    );
    assert!(solutions.next().is_none());
}

#[test]
fn rdflib_exists_preserves_independent_group_wrappers() {
    let store = Store::new().unwrap();
    // bounded: three original-RDFLib EXISTS shapes and two parameter contexts.
    for binding in ["unused", "s"] {
        for (body, expected) in [
            ("BIND(<urn:b> AS ?s)", true),
            ("{ BIND(<urn:b> AS ?s) }", false),
            ("{ SELECT ?s WHERE { BIND(<urn:b> AS ?s) } }", false),
        ] {
            let query = format!("ASK {{ VALUES ?s {{ <urn:a> }} FILTER EXISTS {{ {body} }} }}");
            let QueryResults::Boolean(actual) = SparqlEvaluator::new()
                .with_rdflib_query_context()
                .parse_query(&query)
                .unwrap()
                .with_initial_bindings([(Variable::new(binding).unwrap(), node("a").into())])
                .on_store(&store)
                .execute()
                .unwrap()
            else {
                panic!("Expected ASK")
            };
            assert_eq!(actual, expected, "{binding}: {body}");
        }
    }
}

#[test]
fn rdflib_context_keeps_join_barriers_optional_scope_and_minus_domains() {
    let store = Store::new().unwrap();
    store
        .insert(
            Quad::new(
                node("a"),
                node("p"),
                Literal::from("A"),
                GraphName::DefaultGraph,
            )
            .as_ref(),
        )
        .unwrap();
    store
        .insert(
            Quad::new(
                node("b"),
                node("p"),
                Literal::from("B"),
                GraphName::DefaultGraph,
            )
            .as_ref(),
        )
        .unwrap();
    // bounded: reference-derived algebra cases, each with at most two rows.
    for (query, expected) in [
        (
            "SELECT ?s WHERE { VALUES ?s { <urn:a> } { SELECT DISTINCT ?s WHERE { BIND(<urn:b> AS ?s) } } }",
            None,
        ),
        (
            "SELECT ?s WHERE { VALUES ?s { <urn:a> } { SELECT ?s WHERE { BIND(<urn:b> AS ?s) } LIMIT 1 } }",
            None,
        ),
        (
            "SELECT ?o WHERE { VALUES ?s { <urn:a> } OPTIONAL { { SELECT ?o WHERE { ?s <urn:p> ?o } } } }",
            Some(Literal::from("A").into()),
        ),
        (
            "SELECT ?x WHERE { VALUES ?x {1} MINUS { VALUES ?y {2} } }",
            None,
        ),
        (
            "SELECT ?s WHERE { VALUES ?s { <urn:a> } MINUS { { SELECT ?x WHERE { BIND(1 AS ?x) } } } }",
            Some(node("a").into()),
        ),
    ] {
        let QueryResults::Solutions(solutions) = SparqlEvaluator::new()
            .parse_query(query)
            .unwrap()
            .with_initial_bindings([(Variable::new("unused").unwrap(), node("a").into())])
            .on_store(&store)
            .execute()
            .unwrap()
        else {
            panic!("Expected SELECT")
        };
        let values = solutions
            .map(|row| row.unwrap().get(0).unwrap().clone())
            .collect::<Vec<Term>>();
        assert_eq!(values, expected.into_iter().collect::<Vec<_>>(), "{query}");
    }
}
