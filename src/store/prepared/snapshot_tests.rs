use super::*;
use oxigraph::model::{GraphName, Literal, NamedNode, Quad};
use oxigraph::sparql::QueryResults;
use oxigraph::store::Store;

#[derive(Deserialize)]
struct Case {
    name: String,
    snapshot: String,
    count: usize,
    source: Option<String>,
    bindings: Option<String>,
    data: Option<Vec<[String; 3]>>,
}

#[test]
fn prepared_forms_and_supplied_metadata_use_owned_native_algebra() {
    let cases: Vec<Case> = serde_json::from_str(include_str!(
        "../../../tests/fixtures/prepared_queries.json"
    ))
    .unwrap();
    for case in cases {
        let store = Store::new().unwrap();
        // bounded: three explicit triples in each synthetic query fixture.
        let triples = case.data.unwrap_or_else(|| {
            vec![
                ["a".into(), "p".into(), "A".into()],
                ["b".into(), "p".into(), "B".into()],
                ["a".into(), "q".into(), "yes".into()],
            ]
        });
        for [subject, predicate, object] in triples {
            store
                .insert(
                    Quad::new(
                        NamedNode::new(format!("urn:{subject}")).unwrap(),
                        NamedNode::new(format!("urn:{predicate}")).unwrap(),
                        Literal::new_simple_literal(object),
                        GraphName::DefaultGraph,
                    )
                    .as_ref(),
                )
                .unwrap();
        }
        let bindings =
            super::super::super::bindings::InitialBindings::parse(case.bindings.as_deref())
                .unwrap();
        let (query, _) = prepare(&case.snapshot, false, bindings.names()).unwrap();
        let query = bindings.apply(query).unwrap();
        let actual = match query.on_store(&store).execute().unwrap() {
            QueryResults::Boolean(value) => usize::from(value),
            QueryResults::Solutions(values) => values
                .collect::<std::result::Result<Vec<_>, _>>()
                .unwrap()
                .len(),
            QueryResults::Graph(values) => values
                .collect::<std::result::Result<Vec<_>, _>>()
                .unwrap()
                .len(),
        };
        assert_eq!(actual, case.count, "{}", case.name);
        if let Some(source) = &case.source {
            let query = SparqlEvaluator::new()
                .with_rdflib_query_context()
                .parse_query(source)
                .unwrap();
            let query =
                super::super::super::bindings::apply(query, case.bindings.as_deref()).unwrap();
            let actual = match query.on_store(&store).execute().unwrap() {
                QueryResults::Boolean(value) => usize::from(value),
                QueryResults::Solutions(values) => values
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .unwrap()
                    .len(),
                QueryResults::Graph(values) => values
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .unwrap()
                    .len(),
            };
            assert_eq!(actual, case.count, "source {}", case.name);
        }
    }
}
