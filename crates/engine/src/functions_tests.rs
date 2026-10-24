use super::*;
use oxigraph::io::RdfFormat;
use oxigraph::sparql::SparqlEvaluator;

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

#[test]
fn function_declarations_reject_missing_ambiguous_and_mismatched_queries() {
    for body in [
        "ex:f a sh:SPARQLFunction.",
        "ex:f a sh:SPARQLFunction; sh:select 'SELECT ?x WHERE {}'; sh:ask 'ASK {}'.",
        "ex:f a sh:SPARQLFunction; sh:select 'INVALID'.",
        "ex:f a sh:SPARQLFunction; sh:select 'ASK {}'.",
        "ex:f a sh:SPARQLFunction; sh:ask 'SELECT ?x WHERE {}'.",
        "ex:f a sh:SPARQLFunction; sh:select 'SELECT * WHERE {}'.",
        "ex:f a sh:SPARQLFunction; sh:select 'SELECT ?x WHERE {}'; sh:parameter [].",
        "ex:f a sh:SPARQLFunction; sh:select 'SELECT ?x WHERE {}'; sh:parameter [sh:path <https://example.org/bad-name>].",
        "ex:f a sh:SPARQLFunction; sh:select 'SELECT ?x WHERE {}'; sh:parameter [sh:path ex:x; sh:order ex:nonNumeric].",
        "ex:f a sh:SPARQLFunction; sh:select 'SELECT ?x WHERE {}'; sh:parameter [sh:path ex:x; sh:order 'invalid'].",
    ] {
        assert!(Functions::compile(&graph(body), &[]).is_err(), "{body}");
    }
}

#[test]
fn optional_arguments_and_empty_function_results_have_explicit_semantics() {
    let data = graph(
        "ex:f a sh:SPARQLFunction; sh:select 'SELECT ?x WHERE {}'; sh:parameter [sh:path ex:x; sh:optional true]. ex:ask a sh:SPARQLFunction; sh:ask 'ASK {?s ?p ?o}'. ex:empty a sh:SPARQLFunction; sh:select 'SELECT ?x WHERE {FILTER(false)}'.",
    );
    let functions = Functions::compile(&data, &[]).unwrap();
    let f = iri("https://example.org/", "f");
    assert_eq!(functions.call(&f, &[], &data).unwrap(), None);
    assert_eq!(
        functions
            .call(&f, &[Literal::from(2).into()], &data)
            .unwrap(),
        Some(Literal::from(2).into())
    );
    assert!(
        functions
            .call(
                &f,
                &[Literal::from(1).into(), Literal::from(2).into()],
                &data
            )
            .is_err()
    );
    assert_eq!(
        functions
            .call(&iri("https://example.org/", "empty"), &[], &data)
            .unwrap(),
        None
    );
    assert_eq!(
        functions
            .call(&iri("https://example.org/", "ask"), &[], &data)
            .unwrap(),
        Some(Literal::from(true).into())
    );
    assert!(functions.call(&iri("urn:", "unknown"), &[], &data).is_err());
    assert!(
        functions
            .call_inner(&f, &[], &data, 64)
            .unwrap_err()
            .to_string()
            .contains("recursion limit")
    );
}

#[test]
fn checked_iterators_deliver_pending_errors_once_and_remain_exhausted() {
    let mut checked = Checked {
        inner: vec![Ok(1), Ok(2)].into_iter(),
        errors: Arc::new(Mutex::new(Some(Error::Invalid("callback failed".into())))),
        done: false,
    };
    assert!(
        checked
            .next()
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("callback failed")
    );
    assert!(checked.next().is_none());
    assert!(checked.next().is_none());
    let mut checked = Checked {
        inner: vec![Ok(1)].into_iter(),
        errors: Arc::new(Mutex::new(None)),
        done: false,
    };
    assert_eq!(checked.next().unwrap().unwrap(), 1);
    assert!(checked.next().is_none());
    assert!(checked.next().is_none());
}

#[test]
fn regex_backed_graph_queries_use_checked_triple_results() {
    let data = Dataset::new();
    let QueryResults::Graph(triples) = evaluate_query(
        &data,
        &data,
        "CONSTRUCT { <urn:s> <urn:p> ?v } WHERE { BIND(REPLACE('abc', '(a)', '$1x') AS ?v) }",
        &[],
    )
    .unwrap() else {
        panic!("CONSTRUCT result expected")
    };
    let triples = triples.collect::<std::result::Result<Vec<_>, _>>().unwrap();
    assert_eq!(triples.len(), 1);
    assert_eq!(
        triples[0].object,
        Literal::new_simple_literal("axbc").into()
    );
    let prepared = NativePrepared {
        functions: Functions::default(),
        depth: 0,
        query: SparqlEvaluator::new().parse_query("ASK {}").unwrap(),
        errors: Some(Arc::new(Mutex::new(Some(Error::Invalid(
            "already failed".into(),
        ))))),
    };
    assert!(
        prepared
            .on_queryable_dataset(&data)
            .execute()
            .err()
            .unwrap()
            .to_string()
            .contains("already failed")
    );
}

#[test]
fn external_bindings_survive_every_query_kind_and_projection_modifier() {
    let binding = Variable::new("supplied").unwrap();
    let existing = Variable::new("s").unwrap();
    for text in [
        "SELECT DISTINCT ?s WHERE {?s ?p ?o} ORDER BY ?s LIMIT 3 OFFSET 1",
        "SELECT REDUCED ?s WHERE {?s ?p ?o}",
        "ASK {?s ?p ?o}",
        "CONSTRUCT {?s ?p ?o} WHERE {?s ?p ?o}",
        "DESCRIBE ?s WHERE {?s ?p ?o}",
    ] {
        let mut query = SparqlParser::new().parse_query(text).unwrap();
        project_bindings(
            &mut query,
            &[existing.clone(), binding.clone(), binding.clone()],
        );
        let (Query::Select { pattern, .. }
        | Query::Ask { pattern, .. }
        | Query::Construct { pattern, .. }
        | Query::Describe { pattern, .. }) = &query;
        let mut variables = Vec::new();
        pattern.on_in_scope_variable(|v| variables.push(v.clone()));
        assert!(variables.contains(&existing), "{text}");
        assert_eq!(
            variables.iter().filter(|v| **v == binding).count(),
            1,
            "{text}"
        );
        assert!(SparqlParser::new().parse_query(&query.to_string()).is_ok());
    }
    let data = graph("ex:f sh:parameter [sh:path ex:z], [sh:path ex:a].");
    let lowered =
        lower_parameters(read_parameters(&data, &iri("https://example.org/", "f").into()).unwrap())
            .unwrap();
    assert_eq!(
        lowered
            .iter()
            .map(|p| p.variable.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "z"]
    );
}

#[test]
fn shacl_functions_borrow_pending_transaction_and_nested_function_reads() {
    let shapes = graph(
        "ex:lookup a sh:SPARQLFunction; sh:parameter [ sh:path ex:x ]; sh:select 'SELECT ?result WHERE { $x <https://example.org/value> ?result }'. ex:outer a sh:SPARQLFunction; sh:parameter [ sh:path ex:x ]; sh:select 'SELECT (<https://example.org/lookup>($x) AS ?result) WHERE {}'.",
    );
    let functions = Functions::compile(&shapes, &[]).unwrap();
    let store = oxigraph::store::Store::new().unwrap();
    let subject = iri("https://example.org/", "s");
    let predicate = iri("https://example.org/", "value");
    let old = Quad::new(
        subject.clone(),
        predicate.clone(),
        Literal::from(0),
        GraphName::DefaultGraph,
    );
    store.insert(&old).unwrap();
    let mut transaction = store.start_transaction().unwrap();
    transaction.remove(&old);
    let exact = Literal::new_typed_literal("01", iri("urn:", "exact-datatype"));
    transaction.insert(&Quad::new(
        subject.clone(),
        predicate,
        exact.clone(),
        GraphName::DefaultGraph,
    ));
    for function in ["lookup", "outer"] {
        assert_eq!(
            functions
                .call(
                    &iri("https://example.org/", function),
                    &[subject.clone().into()],
                    &transaction
                )
                .unwrap(),
            Some(exact.clone().into())
        );
    }
    drop(transaction);
    assert!(store.contains(&old).unwrap());
    assert_eq!(store.len().unwrap(), 1);
}

#[test]
fn shared_evaluator_profile_preserves_term_builtins_and_rdf_union_bags() {
    let data = crate::parse_dataset("@prefix xsd: <http://www.w3.org/2001/XMLSchema#>. <urn:g1> { <urn:s> <urn:p> '01'^^xsd:integer. <urn:t> <urn:p> '01'^^xsd:integer. } <urn:g2> { <urn:s> <urn:p> '01'^^xsd:integer. }", RdfFormat::TriG, None).unwrap();
    let QueryResults::Solutions(rows) = evaluate_query(&data, &Dataset::new(), "SELECT (STR(?o) AS ?text) (DATATYPE(?o) AS ?datatype) FROM <urn:g1> FROM <urn:g2> WHERE { ?s <urn:p> ?o }", &[]).unwrap() else { panic!("expected solutions") };
    let rows = rows.map(std::result::Result::unwrap).collect::<Vec<_>>();
    assert_eq!(
        rows.len(),
        2,
        "RDF union removes repeated source triples while distinct triples retain solution bags"
    );
    for row in rows {
        assert_eq!(
            row.get("text"),
            Some(&Literal::new_simple_literal("01").into())
        );
        assert_eq!(row.get("datatype"), Some(&iri(XSD, "integer").into()));
    }
}
