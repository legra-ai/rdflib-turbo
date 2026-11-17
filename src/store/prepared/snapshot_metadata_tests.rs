use super::*;
use oxigraph::store::Store;

#[test]
fn prepared_metadata_cannot_be_omitted_or_rebound_to_another_query() {
    let missing = r#"{"form":"select","base":null,"pattern":{"kind":"optional","left":{"kind":"bgp","triples":[]},"right":{"kind":"bgp","triples":[]},"expression":{"kind":"atom","value":{"term":{"type":"literal","value":"true","datatype":"http://www.w3.org/2001/XMLSchema#boolean"}}}},"template":[],"describe":[],"dataset":[]}"#;
    assert!(
        prepare(missing, false, std::iter::empty()).is_err(),
        "missing source-vars metadata is not absent source-vars metadata"
    );
    let plan = RdfLibPreparedAlgebra::new(GraphPattern::empty_singleton());
    let other = spargebra::SparqlParser::new()
        .parse_query("SELECT ?s WHERE {?s <urn:p> ?o}")
        .unwrap();
    let query = SparqlEvaluator::new()
        .for_query(other)
        .with_rdflib_prepared_algebra(plan);
    assert!(query.on_store(&Store::new().unwrap()).execute().is_err());
}
#[test]
fn prepared_query_configuration_is_rejected_for_update_before_mutations() {
    let configured = spareval::QueryEvaluator::new()
        .with_rdflib_prepared_algebra(RdfLibPreparedAlgebra::new(GraphPattern::empty_singleton()));
    let pattern = spargebra::algebra::GraphPattern::Project {
        inner: Box::new(spargebra::algebra::GraphPattern::Bgp {
            patterns: Vec::new(),
        }),
        variables: Vec::new(),
    };
    let dataset = oxigraph::model::Dataset::new();
    let result = configured
        .prepare_delete_insert(Vec::new(), Vec::new(), None, None, &pattern)
        .execute(&dataset);
    assert!(
        result.is_err(),
        "a prepared query plan must never select an UPDATE WHERE plan"
    );
}
