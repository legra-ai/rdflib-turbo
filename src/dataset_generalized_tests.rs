//! Consumer contracts for the generalized legacy owner; no narrowing or base copies.
use super::*;

fn node(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("urn:consumer:{name}"))
}

fn owner(rows: impl IntoIterator<Item = GeneralizedQuad>) -> NativeDataset {
    let data: GeneralizedDataset = rows.into_iter().collect();
    NativeDataset {
        inner: RwLock::new(DatasetState::new(data, Vec::new())),
    }
}

#[test]
fn owner_accepts_parser_data_and_retains_generalized_json_roles() {
    let strict = Dataset::from_iter([Quad::new(
        node("s"),
        node("p"),
        node("o"),
        GraphName::DefaultGraph,
    )]);
    let strict = DatasetState::new(strict, Vec::new());
    let _: &GeneralizedDataset = &strict;
    assert_eq!(strict.len(), 1);
    let data = owner([GeneralizedQuad::new(
        Literal::new_typed_literal("01", node("datatype")),
        Literal::new_simple_literal("predicate"),
        node("o"),
        GraphName::DefaultGraph,
    )]);
    Python::initialize();
    Python::attach(|py| {
        let rows: Value = serde_json::from_str(&data.quads_json(py).unwrap()).unwrap();
        assert_eq!(rows[0][0]["type"], "literal");
        assert_eq!(rows[0][0]["value"], "01");
        assert_eq!(rows[0][0]["datatype"], "urn:consumer:datatype");
        assert_eq!(rows[0][1]["type"], "literal");
        assert_eq!(data.query(py, "ASK { ?s ?p ?o }", false).unwrap().1, "true");
    });
}

#[test]
fn strict_dump_errors_without_filtering_or_mutating_generalized_positions() {
    let literal = Literal::new_simple_literal("wide".repeat(16_384));
    for (subject, predicate) in [
        (Term::from(literal.clone()), Term::from(node("p"))),
        (Term::from(node("s")), Term::from(literal.clone())),
        (
            Term::from(node("s")),
            Term::from(BlankNode::new("predicate").unwrap()),
        ),
    ] {
        let data = owner([GeneralizedQuad::new(
            subject,
            predicate,
            node("o"),
            GraphName::DefaultGraph,
        )]);
        Python::initialize();
        Python::attach(|py| {
            let before = data.quads_json(py).unwrap();
            let error = data.dump(py, "nquads").unwrap_err();
            assert!(error.is_instance_of::<PyValueError>(py));
            assert!(
                error.to_string().len() < 256,
                "do not copy a wide literal into an error"
            );
            assert_eq!(data.quads_json(py).unwrap(), before);
            assert_eq!(data.__len__().unwrap(), 1);
        });
    }
}

#[test]
fn failed_parser_load_keeps_existing_generalized_rows_and_revision() {
    let data = owner([GeneralizedQuad::new(
        Literal::new_simple_literal("subject"),
        node("p"),
        node("o"),
        GraphName::DefaultGraph,
    )]);
    Python::initialize();
    Python::attach(|py| {
        let before = data.quads_json(py).unwrap();
        assert!(
            data.load(py, "<urn:new> <urn:p> <urn:o> .\nnot-rdf", "nt", None, true)
                .is_err()
        );
        assert_eq!(data.quads_json(py).unwrap(), before);
        assert_eq!(data.inner.read().unwrap().revision, 0);
    });
}

#[test]
fn union_query_retains_default_named_and_from_scope_without_duplicating_rows() {
    let s = node("s");
    let p = node("p");
    let g = node("graph");
    let common = node("common");
    let data = owner([
        GeneralizedQuad::new(
            s.clone(),
            p.clone(),
            common.clone(),
            GraphName::DefaultGraph,
        ),
        GeneralizedQuad::new(s.clone(), p.clone(), common, g.clone()),
        GeneralizedQuad::new(s.clone(), p.clone(), node("named"), g.clone()),
        GeneralizedQuad::new(s, p, node("blank"), BlankNode::new("graph").unwrap()),
    ]);
    Python::initialize();
    Python::attach(|py| {
        for (query, union, count) in [
            ("SELECT (COUNT(*) AS ?n) WHERE { ?s ?p ?o }", false, "1"),
            ("SELECT (COUNT(*) AS ?n) WHERE { ?s ?p ?o }", true, "3"),
            (
                "SELECT (COUNT(*) AS ?n) FROM <urn:consumer:graph> WHERE { ?s ?p ?o }",
                true,
                "2",
            ),
            (
                "SELECT (COUNT(*) AS ?n) WHERE { GRAPH <urn:consumer:graph> { ?s ?p ?o } }",
                true,
                "2",
            ),
            (
                "SELECT (COUNT(*) AS ?n) WHERE { GRAPH ?g { ?s ?p ?o } FILTER(isBlank(?g)) }",
                true,
                "1",
            ),
        ] {
            let result: Value =
                serde_json::from_str(&data.query(py, query, union).unwrap().1).unwrap();
            assert_eq!(result["rows"][0]["n"]["value"], count, "{query}");
        }
        assert_eq!(data.__len__().unwrap(), 4);
    });
}
