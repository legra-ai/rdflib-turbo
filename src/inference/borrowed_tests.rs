//! Private inference overlays retain exact scope and only newly derived facts.
use oxigraph::model::*;
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;
use pyshacl_turbo_engine::{Error as ReadError, GraphRead, QueryGraph};

use super::backend::Graph;
use super::borrowed::BorrowedInference;

fn node(value: &str) -> NamedNode {
    NamedNode::new(format!("urn:{value}")).unwrap()
}

fn quad(subject: &str, predicate: &str, object: &str, graph: GraphName) -> Quad {
    Quad::new(node(subject), node(predicate), node(object), graph)
}

#[test]
fn borrowed_overlay_queries_one_logical_union_and_retains_physical_contexts() {
    let store = Store::new().unwrap();
    let mut transaction = store.start_transaction().unwrap();
    transaction.insert(&quad("s", "p", "o", GraphName::DefaultGraph));
    transaction.insert(&quad("s", "p", "o", node("named").into()));
    transaction.insert(&quad("other", "p", "o", node("named").into()));
    transaction.insert_named_graph(&node("empty"));
    let mut data = BorrowedInference::new(&transaction, node("inference").into());
    data.insert(quad("derived", "p", "o", GraphName::DefaultGraph))
        .unwrap();
    assert_eq!(data.quads(None, None, None).count(), 3);
    let query = SparqlEvaluator::new()
        .parse_query("SELECT ?s WHERE {?s <urn:p> <urn:o>}")
        .unwrap();
    let QueryResults::Solutions(rows) = data.query(query).unwrap() else {
        panic!("SELECT")
    };
    assert_eq!(
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .unwrap()
            .len(),
        3
    );
    assert!(data.contains_named_graph(&node("empty").into()).unwrap());
    assert!(
        data.contains_named_graph(&node("inference").into())
            .unwrap()
    );
    assert_eq!(
        data.matching_quads(None, None, None, Some(node("named").as_ref().into()))
            .count(),
        2
    );
    assert_eq!(
        data.matching_quads(None, None, None, Some(node("inference").as_ref().into()))
            .count(),
        1
    );
    assert_eq!(
        data.named_graphs()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(data.into_additions().len(), 1);
    assert_eq!(transaction.len().unwrap(), 3);
}

#[test]
fn scoped_overlay_does_not_read_tempting_schema_from_another_graph() {
    let store = Store::new().unwrap();
    let mut transaction = store.start_transaction().unwrap();
    let selected = BlankNode::new("selected").unwrap();
    transaction.insert(&quad("s", "p", "o", selected.clone().into()));
    transaction.insert(&quad("wrong", "p", "o", node("outside").into()));
    let scoped = pyshacl_turbo_engine::ScopedGraph::new(&transaction, selected.into());
    let mut data = BorrowedInference::new(&scoped, GraphName::DefaultGraph);
    data.insert(quad("derived", "p", "o", GraphName::DefaultGraph))
        .unwrap();
    assert_eq!(data.quads(None, None, None).count(), 2);
    let query = SparqlEvaluator::new()
        .parse_query("ASK {<urn:wrong> ?p ?o}")
        .unwrap();
    let QueryResults::Boolean(result) = data.query(query).unwrap() else {
        panic!("ASK")
    };
    assert!(!result);
    assert_eq!(data.named_graphs().count(), 0);
    assert_eq!(data.into_additions().len(), 1);
    assert_eq!(transaction.len().unwrap(), 2);
}

struct FailedLookup;

impl GraphRead for FailedLookup {
    fn matching_quads<'a>(
        &'a self,
        _: Option<NamedOrBlankNodeRef<'_>>,
        _: Option<NamedNodeRef<'_>>,
        _: Option<TermRef<'_>>,
        _: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = pyshacl_turbo_engine::Result<Quad>> + use<'a> {
        std::iter::once(Err(ReadError::Runtime("source lookup failed".into())))
    }
    fn named_graphs(
        &self,
    ) -> impl Iterator<Item = pyshacl_turbo_engine::Result<NamedOrBlankNode>> + '_ {
        std::iter::empty()
    }
    fn contains_named_graph(&self, _: &NamedOrBlankNode) -> pyshacl_turbo_engine::Result<bool> {
        Err(ReadError::Runtime("source metadata failed".into()))
    }
}

#[test]
fn derived_insertion_propagates_borrowed_lookup_errors_without_sticky_state() {
    let mut data = BorrowedInference::new(&FailedLookup, node("inference").into());
    let error = data
        .insert(quad("derived", "p", "o", GraphName::DefaultGraph))
        .unwrap_err();
    assert_eq!(error.to_string(), "source lookup failed");
    let query = SparqlEvaluator::new()
        .parse_query("ASK {?s ?p ?o}")
        .unwrap();
    assert!(
        query
            .on_queryable_dataset(QueryGraph::new(&data))
            .execute()
            .is_err()
    );
    assert_eq!(data.into_additions().len(), 0);
}

#[cfg(feature = "allocation-profiling")]
#[test]
#[ignore = "global allocation counters require isolated execution"]
fn borrowed_validation_view_does_not_clone_large_base_lexical_storage() {
    let store = Store::new().unwrap();
    let mut source = store.start_transaction().unwrap();
    let lexical = "x".repeat(4096);
    // bounded: fixed 512-row synthetic source, over 2MiB of unique lexical storage.
    for index in 0..512 {
        source.insert(&Quad::new(
            NamedNode::new(format!("urn:{index}:{lexical}")).unwrap(),
            node("p"),
            node("o"),
            GraphName::DefaultGraph,
        ));
    }
    let shapes = pyshacl_turbo_engine::CompiledShapes::compile(
        pyshacl_turbo_engine::parse_graph("", oxigraph::io::RdfFormat::Turtle, None).unwrap(),
        false,
    )
    .unwrap();
    let ((), bytes) = crate::allocation_tests::measure_allocated(|| {
        let mut data = BorrowedInference::new(&source, GraphName::DefaultGraph);
        // Isolate the base-view/staging seam with a zero-entailment request.
        // Real active closure/query allocations are deliberately outside this budget.
        data.expand("none", super::InferenceRequest::default())
            .unwrap();
        let result = shapes
            .validate_delta(&data, pyshacl_turbo_engine::Options::default())
            .unwrap();
        assert!(result.report.conforms);
        assert!(result.additions.is_empty());
        assert!(data.into_additions().is_empty());
    });
    println!(
        "BORROWED_VALIDATION_VIEW allocated_bytes={bytes} base_rows=512 lexical_bytes_min=2097152"
    );
    assert!(
        bytes < 16 * 1024,
        "Base view copied lexical storage: {bytes}"
    );
    assert_eq!(source.len().unwrap(), 512);
}
