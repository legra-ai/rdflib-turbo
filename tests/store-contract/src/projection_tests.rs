use oxigraph::model::{BlankNode, GraphName, GraphNameRef, Literal, NamedNode, Quad, Term};
use oxigraph::store::Store;

fn node(value: &str) -> NamedNode {
    NamedNode::new_unchecked(value)
}

#[test]
fn all_projection_masks_preserve_terms_and_graph_scope() {
    let store = Store::new().unwrap();
    let datatype = node("http://www.w3.org/2001/XMLSchema#integer");
    let named = node("urn:g");
    // bounded: five fixed statements exercising distinct RDF term forms.
    let quads = [
        Quad::new(
            node("relative"),
            node("urn:p"),
            Literal::new_typed_literal("01", datatype),
            GraphName::DefaultGraph,
        ),
        Quad::new(
            BlankNode::new_unchecked("subject"),
            node("urn:p"),
            Literal::new_language_tagged_literal("hello", "en").unwrap(),
            GraphName::DefaultGraph,
        ),
        Quad::new(
            node("urn:s"),
            node("urn:p"),
            Literal::new_simple_literal("plain"),
            GraphName::DefaultGraph,
        ),
        Quad::new(
            node("urn:s"),
            node("urn:p"),
            Literal::new_typed_literal("typed", node("http://www.w3.org/2001/XMLSchema#string")),
            named.clone(),
        ),
        Quad::new(
            node("urn:s"),
            node("urn:p"),
            BlankNode::new_unchecked("object"),
            named.clone(),
        ),
    ];
    for quad in &quads {
        store.insert(quad).unwrap();
    }
    for graph in [GraphNameRef::DefaultGraph, named.as_ref().into()] {
        for mask in 0_u8..8 {
            let selected = [mask & 1 != 0, mask & 2 != 0, mask & 4 != 0];
            // bounded: at most the five fixed fixture statements.
            let expected: Vec<_> = store
                .quads_for_pattern(None, None, None, Some(graph))
                .map(|quad| {
                    let quad = quad.unwrap();
                    [
                        selected[0].then(|| Term::from(quad.subject)),
                        selected[1].then(|| Term::from(quad.predicate)),
                        selected[2].then_some(quad.object),
                    ]
                })
                .collect();
            let actual: Vec<_> = store
                .quads_for_pattern(None, None, None, Some(graph))
                .project(selected[0], selected[1], selected[2])
                .map(Result::unwrap)
                .collect();
            assert_eq!(actual, expected, "mask {mask}, graph {graph}");
        }
    }
    #[cfg(feature = "exact")]
    {
        let projected = store
            .quads_for_pattern(
                Some(node("relative").as_ref().into()),
                None,
                None,
                Some(GraphNameRef::DefaultGraph),
            )
            .project(false, false, true)
            .next()
            .unwrap()
            .unwrap();
        let Some(Term::Literal(value)) = &projected[2] else {
            panic!("Expected literal")
        };
        assert_eq!(value.value(), "01");
    }
}

#[test]
fn projected_iterator_owns_the_original_snapshot() {
    let store = Store::new().unwrap();
    let before = Quad::new(
        node("urn:before"),
        node("urn:p"),
        node("urn:o"),
        GraphName::DefaultGraph,
    );
    store.insert(&before).unwrap();
    let mut projected = store.iter().project(true, false, false);
    store
        .insert(&Quad::new(
            node("urn:after"),
            node("urn:p"),
            node("urn:o"),
            GraphName::DefaultGraph,
        ))
        .unwrap();
    drop(store);
    assert_eq!(
        projected.next().unwrap().unwrap(),
        [Some(node("urn:before").into()), None, None]
    );
    assert!(projected.next().is_none());
}

#[test]
fn projection_can_skip_a_large_object_and_keeps_transaction_borrowing() {
    let store = Store::new().unwrap();
    let mut transaction = store.start_transaction().unwrap();
    // bounded: one deliberately large fixture literal, not a dataset-sized materialization.
    let object = Literal::new_simple_literal("x".repeat(1024 * 1024));
    transaction.insert(&Quad::new(
        node("urn:s"),
        node("urn:p"),
        object,
        node("urn:g"),
    ));
    let mut projected = transaction.iter().project(true, true, false);
    assert_eq!(
        projected.next().unwrap().unwrap(),
        [Some(node("urn:s").into()), Some(node("urn:p").into()), None]
    );
    assert!(projected.next().is_none());
    drop(projected);
    drop(transaction);
    assert!(store.is_empty().unwrap());
}
