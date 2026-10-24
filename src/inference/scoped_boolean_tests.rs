//! Boolean suppression reads the selected source or destination, never unrelated scopes.
use oxigraph::model::*;
use oxigraph::store::Store;

use super::super::backend::Graph;
use super::ScopedGraph;

fn node(value: &str) -> NamedNode {
    NamedNode::new(value).unwrap()
}
fn quad(graph: GraphName) -> Quad {
    Quad::new(
        node("urn:s"),
        node("urn:p"),
        Literal::new_typed_literal("01", node("http://www.w3.org/2001/XMLSchema#integer")),
        graph,
    )
}

#[test]
fn scoped_boolean_membership_keeps_source_or_destination_and_exact_graph_kinds() {
    let source: GraphName = node("urn:scope").into();
    let destination: GraphName = BlankNode::new_unchecked("urn:scope").into();
    for physical in [
        source.clone(),
        destination.clone(),
        GraphName::DefaultGraph,
        node("urn:unrelated").into(),
    ] {
        let store = Store::new().unwrap();
        let mut transaction = store.start_transaction().unwrap();
        transaction.insert(&quad(physical.clone()));
        let data = ScopedGraph {
            transaction: &mut transaction,
            source: source.clone(),
            destination: destination.clone(),
        };
        let candidate = quad(GraphName::DefaultGraph);
        assert_eq!(
            data.contains((&candidate).into()).unwrap(),
            physical == source || physical == destination
        );
        assert_eq!(
            data.contains_destination((&candidate).into()).unwrap(),
            physical == destination
        );
        let mut other = candidate.clone();
        other.object =
            Literal::new_typed_literal("1", node("http://www.w3.org/2001/XMLSchema#integer"))
                .into();
        assert!(!data.contains((&other).into()).unwrap());
    }
}

#[test]
fn scoped_boolean_membership_follows_pending_writes_and_rollback() {
    let store = Store::new().unwrap();
    let source: GraphName = node("urn:source").into();
    let destination: GraphName = node("urn:destination").into();
    {
        let mut transaction = store.start_transaction().unwrap();
        transaction.insert(&quad(destination.clone()));
        let data = ScopedGraph {
            transaction: &mut transaction,
            source: source.clone(),
            destination: destination.clone(),
        };
        assert!(
            data.contains((&quad(GraphName::DefaultGraph)).into())
                .unwrap()
        );
        assert!(
            data.contains_destination((&quad(GraphName::DefaultGraph)).into())
                .unwrap()
        );
        assert!(store.is_empty().unwrap());
    }
    let mut transaction = store.start_transaction().unwrap();
    let data = ScopedGraph {
        transaction: &mut transaction,
        source,
        destination,
    };
    assert!(
        !data
            .contains((&quad(GraphName::DefaultGraph)).into())
            .unwrap()
    );
    assert!(
        !data
            .contains_destination((&quad(GraphName::DefaultGraph)).into())
            .unwrap()
    );
}

#[cfg(feature = "allocation-profiling")]
#[test]
#[ignore = "global allocation counters require isolated execution"]
fn scoped_boolean_probes_do_not_clone_bound_lexical_terms() {
    let text = "x".repeat(64 * 1024);
    let input = Quad::new(
        node(&format!("urn:s:{text}")),
        node(&format!("urn:p:{text}")),
        Literal::new_simple_literal(&text),
        node(&format!("urn:g:{text}")),
    );
    let destination: GraphName = node(&format!("urn:destination:{text}")).into();
    let store = Store::new().unwrap();
    let mut transaction = store.start_transaction().unwrap();
    transaction.insert(&input);
    let mut derived = input.clone();
    derived.graph_name = destination.clone();
    transaction.insert(&derived);
    let data = ScopedGraph {
        transaction: &mut transaction,
        source: input.graph_name.clone(),
        destination,
    };
    assert!(data.contains((&input).into()).unwrap()); // warm first-touch synchronization outside measurement.
    let (present, allocated) =
        crate::allocation_tests::measure_allocated(|| data.contains((&input).into()).unwrap());
    assert!(present);
    assert_eq!(
        allocated, 0,
        "source-or-destination boolean cloned lexical payload"
    );
    assert!(data.contains_destination((&input).into()).unwrap()); // warm destination first touch.
    let (present, allocated) = crate::allocation_tests::measure_allocated(|| {
        data.contains_destination((&input).into()).unwrap()
    });
    assert!(present);
    assert_eq!(
        allocated, 0,
        "destination boolean cloned bound lexical payload"
    );
}
