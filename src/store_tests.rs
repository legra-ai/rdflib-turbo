//! Regression gate before moving the shared runtime onto Oxigraph Store.

use oxigraph::model::{GraphName, Literal, NamedNode, Quad, Term, Variable};
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;

fn node(value: &str) -> NamedNode {
    NamedNode::new(format!("urn:{value}")).unwrap()
}

fn literal(value: &str, datatype: &str) -> Literal {
    Literal::new_typed_literal(
        value,
        NamedNode::new(format!("http://www.w3.org/2001/XMLSchema#{datatype}")).unwrap(),
    )
}

fn quad(value: Literal) -> Quad {
    Quad::new(node("s"), node("p"), value, GraphName::DefaultGraph)
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

#[test]
fn store_keeps_numeric_spellings_and_declared_datatypes() {
    let store = Store::new().unwrap();
    let values = [
        literal("01", "integer"),
        literal("1", "integer"),
        literal("4.0", "decimal"),
        literal("300", "byte"),
        literal("+1", "int"),
    ];
    for value in &values {
        store.insert(&quad(value.clone())).unwrap();
    }
    assert_eq!(store.len().unwrap(), values.len());
    for value in values {
        assert!(
            store
                .iter()
                .any(|entry| entry.unwrap().object == Term::Literal(value.clone()))
        );
    }
}

#[test]
fn transaction_abort_and_commit_preserve_exact_terms() {
    let store = Store::new().unwrap();
    store.insert(&quad(literal("01", "integer"))).unwrap();
    {
        let mut transaction = store.start_transaction().unwrap();
        transaction.insert(&quad(literal("1", "integer")));
        assert_eq!(transaction.len().unwrap(), 2);
    }
    assert_eq!(store.len().unwrap(), 1);
    let mut transaction = store.start_transaction().unwrap();
    transaction.insert(&quad(literal("1", "integer")));
    transaction.commit().unwrap();
    assert_eq!(store.len().unwrap(), 2);
}

#[test]
fn select_and_construct_keep_literal_lexical_identity() {
    let store = Store::new().unwrap();
    let value = literal("01", "integer");
    store.insert(&quad(value.clone())).unwrap();
    let QueryResults::Solutions(mut rows) = SparqlEvaluator::new()
        .parse_query("SELECT ?o WHERE { ?s ?p ?o }")
        .unwrap()
        .on_store(&store)
        .execute()
        .unwrap()
    else {
        panic!("SELECT")
    };
    assert_eq!(
        rows.next().unwrap().unwrap().get("o"),
        Some(&Term::Literal(value.clone()))
    );
    let QueryResults::Graph(mut triples) = SparqlEvaluator::new()
        .parse_query("CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }")
        .unwrap()
        .on_store(&store)
        .execute()
        .unwrap()
    else {
        panic!("CONSTRUCT")
    };
    assert_eq!(
        triples.next().unwrap().unwrap().object,
        Term::Literal(value)
    );
}

fn query_value(store: &Store, query: &str, variable: &str) -> Term {
    let QueryResults::Solutions(mut rows) = SparqlEvaluator::new()
        .parse_query(query)
        .unwrap()
        .on_store(store)
        .execute()
        .unwrap()
    else {
        panic!("SELECT")
    };
    let value = rows.next().unwrap().unwrap().get(variable).unwrap().clone();
    assert!(rows.next().is_none());
    value
}

#[test]
fn numeric_value_equality_keeps_distinct_rdf_terms() {
    let store = Store::new().unwrap();
    store.insert(&quad(literal("01", "integer"))).unwrap();
    store.insert(&quad(literal("1", "integer"))).unwrap();
    assert_eq!(
        query_value(
            &store,
            "SELECT (COUNT(?o) AS ?n) WHERE { ?s ?p ?o FILTER(?o = 1) }",
            "n"
        ),
        Term::Literal(literal("2", "integer"))
    );
    assert_eq!(
        query_value(
            &store,
            "SELECT (COUNT(DISTINCT ?o) AS ?n) WHERE { ?s ?p ?o }",
            "n"
        ),
        Term::Literal(literal("2", "integer"))
    );
    assert_eq!(
        query_value(&store, "SELECT (SUM(?o) AS ?n) WHERE { ?s ?p ?o }", "n"),
        Term::Literal(literal("2", "integer"))
    );
}

#[test]
fn computed_numeric_bindings_join_by_exact_term_identity() {
    let store = Store::new().unwrap();
    store.insert(&quad(literal("01", "integer"))).unwrap();
    store.insert(&quad(literal("1", "integer"))).unwrap();
    assert_eq!(
        query_value(
            &store,
            "SELECT (COUNT(*) AS ?n) WHERE { BIND(1 AS ?o) ?s ?p ?o }",
            "n"
        ),
        Term::Literal(literal("1", "integer"))
    );
}

#[test]
fn effective_boolean_value_decodes_exact_stored_literals() {
    let store = Store::new().unwrap();
    for value in [
        literal("01", "integer"),
        literal("0", "integer"),
        literal("1", "boolean"),
        literal("false", "boolean"),
    ] {
        store.insert(&quad(value)).unwrap();
    }
    assert_eq!(
        query_value(
            &store,
            "SELECT (COUNT(*) AS ?n) WHERE { ?s ?p ?o FILTER(?o) }",
            "n"
        ),
        Term::Literal(literal("2", "integer"))
    );
}

#[test]
fn term_builtins_keep_lexical_form_and_declared_datatype() {
    let store = Store::new().unwrap();
    store.insert(&quad(literal("01", "byte"))).unwrap();
    assert_eq!(
        query_value(&store, "SELECT (STR(?o) AS ?s) WHERE { ?s0 ?p ?o }", "s"),
        Term::Literal(Literal::new_simple_literal("01"))
    );
}

#[test]
fn plain_and_explicit_string_terms_remain_distinct() {
    let store = Store::new().unwrap();
    store
        .insert(&quad(Literal::new_simple_literal("value")))
        .unwrap();
    store.insert(&quad(literal("value", "string"))).unwrap();
    assert_eq!(store.len().unwrap(), 2);
    assert_eq!(
        query_value(
            &store,
            "SELECT (COUNT(DISTINCT ?o) AS ?n) WHERE { ?s ?p ?o }",
            "n"
        ),
        Term::Literal(literal("2", "integer"))
    );
}

#[test]
fn sparql_updates_delete_only_the_exact_spelling() {
    let store = Store::new().unwrap();
    SparqlEvaluator::new().parse_update("PREFIX xsd:<http://www.w3.org/2001/XMLSchema#> INSERT DATA { <urn:s> <urn:p> \"01\"^^xsd:integer, \"1\"^^xsd:integer }").unwrap().on_store(&store).execute().unwrap();
    assert_eq!(store.len().unwrap(), 2);
    SparqlEvaluator::new().parse_update("PREFIX xsd:<http://www.w3.org/2001/XMLSchema#> DELETE DATA { <urn:s> <urn:p> \"1\"^^xsd:integer }").unwrap().on_store(&store).execute().unwrap();
    assert_eq!(store.len().unwrap(), 1);
    assert_eq!(
        store.iter().next().unwrap().unwrap().object,
        Term::Literal(literal("01", "integer"))
    );
}

#[test]
fn datatype_builtin_keeps_the_declared_numeric_subtype() {
    let store = Store::new().unwrap();
    store.insert(&quad(literal("01", "byte"))).unwrap();
    assert_eq!(
        query_value(
            &store,
            "SELECT (DATATYPE(?o) AS ?d) WHERE { ?s ?p ?o }",
            "d"
        ),
        Term::NamedNode(NamedNode::new("http://www.w3.org/2001/XMLSchema#byte").unwrap())
    );
}

#[test]
fn term_builtins_preserve_constant_lexical_forms() {
    let store = Store::new().unwrap();
    assert_eq!(
        query_value(
            &store,
            "SELECT (STR(\"01\"^^<http://www.w3.org/2001/XMLSchema#byte>) AS ?v) WHERE {}",
            "v"
        ),
        Term::Literal(Literal::new_simple_literal("01"))
    );
    assert_eq!(
        query_value(
            &store,
            "SELECT (DATATYPE(\"01\"^^<http://www.w3.org/2001/XMLSchema#byte>) AS ?v) WHERE {}",
            "v"
        ),
        Term::NamedNode(NamedNode::new("http://www.w3.org/2001/XMLSchema#byte").unwrap())
    );
}

#[test]
fn transaction_queries_read_uncommitted_changes_without_exposing_them() {
    let store = Store::new().unwrap();
    let mut transaction = store.start_transaction().unwrap();
    transaction.insert(&quad(literal("01", "integer")));
    let QueryResults::Solutions(mut rows) = SparqlEvaluator::new()
        .parse_query("SELECT ?o WHERE { ?s ?p ?o }")
        .unwrap()
        .on_transaction(&transaction)
        .execute()
        .unwrap()
    else {
        panic!("SELECT")
    };
    assert_eq!(
        rows.next().unwrap().unwrap().get("o"),
        Some(&Term::Literal(literal("01", "integer")))
    );
    assert!(rows.next().is_none());
    drop(rows);
    assert_eq!(store.len().unwrap(), 0);
    drop(transaction);
    assert_eq!(store.len().unwrap(), 0);
}

#[test]
fn large_and_invalid_literals_survive_named_graph_storage() {
    let store = Store::new().unwrap();
    let graph = GraphName::NamedNode(node("graph"));
    for value in [
        literal("00000000000000000000000001", "integer"),
        literal("invalid", "boolean"),
    ] {
        let quad = Quad::new(node("s"), node("p"), value.clone(), graph.clone());
        store.insert(&quad).unwrap();
        assert!(store.contains(&quad).unwrap());
        assert!(store.iter().any(|entry| entry.unwrap() == quad));
    }
    assert_eq!(store.len().unwrap(), 2);
}
