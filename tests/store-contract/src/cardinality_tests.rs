use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::{BlankNode, GraphName, GraphNameRef, Literal, NamedNode, Quad};
use oxigraph::store::Store;

fn quad(index: usize, graph: GraphName) -> Quad {
    Quad::new(
        NamedNode::new_unchecked(format!("urn:s{index}")),
        NamedNode::new_unchecked("urn:p"),
        Literal::new_simple_literal("value"),
        graph,
    )
}

fn check(store: &Store) {
    assert_eq!(store.len().unwrap(), store.iter().count());
    assert_eq!(store.is_empty().unwrap(), store.len().unwrap() == 0);
    for graph in [
        GraphName::DefaultGraph,
        NamedNode::new_unchecked("urn:g").into(),
        BlankNode::new_unchecked("graph").into(),
    ] {
        assert_eq!(
            store.graph_len(graph.as_ref()).unwrap(),
            store
                .quads_for_pattern(None, None, None, Some(graph.as_ref()))
                .count()
        );
    }
}

fn exercise(store: &Store) {
    let graphs = [
        GraphName::DefaultGraph,
        NamedNode::new_unchecked("urn:g").into(),
        BlankNode::new_unchecked("graph").into(),
    ];
    for graph in &graphs {
        let value = quad(0, graph.clone());
        store.insert(&value).unwrap();
        store.insert(&value).unwrap();
        check(store);
    }
    assert_eq!(store.len().unwrap(), 3); // Same SPO in three contexts is three quads.
    let mut snapshot = store.iter();
    {
        let mut tx = store.start_transaction().unwrap();
        for graph in &graphs {
            let old = quad(0, graph.clone());
            tx.remove(&old);
            tx.remove(&old);
            tx.insert(&old);
            tx.insert(&old);
            tx.remove(&old);
            tx.insert(&quad(1, graph.clone()));
            assert_eq!(tx.graph_len(graph.as_ref()).unwrap(), 1);
            assert_eq!(tx.len().unwrap(), tx.iter().count());
        }
        assert_eq!(store.len().unwrap(), 3);
        tx.commit().unwrap();
    }
    assert!(snapshot.all(|row| row.unwrap().subject == quad(0, GraphName::DefaultGraph).subject));
    check(store);
    {
        let mut tx = store.start_transaction().unwrap();
        tx.clear_graph(GraphNameRef::DefaultGraph).unwrap();
        tx.insert(&quad(3, GraphName::DefaultGraph));
        tx.clear_graph(GraphNameRef::DefaultGraph).unwrap();
        assert_eq!(tx.graph_len(GraphNameRef::DefaultGraph).unwrap(), 0);
        assert_eq!(tx.len().unwrap(), 2);
    } // Abort leaves all three committed quads.
    assert_eq!(store.len().unwrap(), 3);
    store.clear_graph(GraphNameRef::DefaultGraph).unwrap();
    store
        .remove_named_graph(NamedNode::new_unchecked("urn:g").as_ref())
        .unwrap();
    check(store);
    store.clear().unwrap();
    check(store);
    assert_eq!(store.len().unwrap(), 0);
    let graph = NamedNode::new_unchecked("urn:g");
    store.insert_named_graph(graph.as_ref()).unwrap();
    assert!(store.contains_named_graph(graph.as_ref()).unwrap());
    assert_eq!(store.graph_len(graph.as_ref()).unwrap(), 0);
    let input = b"<urn:s> <urn:p> \"value\" .\n<urn:s> <urn:p> \"value\" .";
    store
        .load_from_reader(RdfParser::from_format(RdfFormat::NTriples), &input[..])
        .unwrap();
    check(store);
    assert_eq!(store.len().unwrap(), 1);
    let mut loader = store.bulk_loader();
    loader
        .load_quads([
            quad(4, GraphName::DefaultGraph),
            quad(4, GraphName::DefaultGraph),
            quad(4, graph.into()),
        ])
        .unwrap();
    loader.commit().unwrap();
    check(store);
    assert_eq!(store.len().unwrap(), 3);
}

#[test]
fn cardinality_memory_mutations_and_transactions() {
    exercise(&Store::new().unwrap());
}

#[test]
fn cardinality_publication_keeps_snapshot_contents_consistent() {
    let store = Store::new().unwrap();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            for i in 0..100 {
                let mut tx = store.start_transaction().unwrap();
                tx.insert(&quad(i, GraphName::DefaultGraph));
                tx.commit().unwrap();
            }
        });
        for _ in 0..100 {
            // Each iterator retains its own committed snapshot throughout consumption.
            let rows = store.iter();
            let values = rows.map(Result::unwrap).count();
            assert!(values <= 100);
        }
    });
    check(&store);
    assert_eq!(store.len().unwrap(), 100);
}

#[cfg(feature = "persistence")]
#[test]
fn cardinality_persistent_exact_scan_and_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    exercise(&store);
    drop(store);
    let store = Store::open_read_only(directory.path()).unwrap();
    check(&store);
    assert_eq!(store.len().unwrap(), 3);
}
