use super::*;

fn quad(object: Term, graph: GraphName) -> Quad {
    Quad::new(
        BlankNode::new("focus").unwrap(),
        NamedNode::new("urn:p").unwrap(),
        object,
        graph,
    )
}

#[test]
fn commit_records_only_new_quads_and_preserves_exact_terms_and_graphs() {
    let plain = quad(
        Literal::new_simple_literal("value").into(),
        GraphName::DefaultGraph,
    );
    let named = quad(
        NamedNode::new("urn:new").unwrap().into(),
        NamedNode::new("urn:g").unwrap().into(),
    );
    let mut data = Dataset::from_iter([plain.clone()]);
    let mut transaction = data.insert_transaction();
    assert!(!transaction.insert(&plain));
    assert!(transaction.insert(&named));
    assert!(!transaction.insert(&named));
    assert_eq!(
        transaction.inserted().collect::<Vec<_>>(),
        vec![named.as_ref()]
    );
    assert_eq!(transaction.len(), 2);
    transaction.commit();
    assert_eq!(data, Dataset::from_iter([plain, named]));
}

#[test]
fn dropping_a_transaction_restores_every_index_and_releases_new_interned_terms() {
    let original = quad(
        Literal::new_typed_literal(
            "01",
            NamedNode::new("http://www.w3.org/2001/XMLSchema#integer").unwrap(),
        )
        .into(),
        GraphName::DefaultGraph,
    );
    let added = quad(
        NamedNode::new("urn:temporary").unwrap().into(),
        NamedNode::new("urn:temporary-graph").unwrap().into(),
    );
    let mut data = Dataset::from_iter([original.clone()]);
    {
        let mut transaction = data.insert_transaction();
        transaction.insert(&added);
        assert!(transaction.contains(&original));
        assert!(transaction.contains(&added));
    }
    assert_eq!(data, Dataset::from_iter([original.clone()]));
    assert!(
        data.encoded_named_node(NamedNodeRef::new_unchecked("urn:temporary"))
            .is_none()
    );
    assert!(
        data.encoded_named_node(NamedNodeRef::new_unchecked("urn:temporary-graph"))
            .is_none()
    );
    assert_eq!(data.quads_for_subject(original.subject.as_ref()).count(), 1);
    assert_eq!(
        data.quads_for_predicate(original.predicate.as_ref())
            .count(),
        1
    );
    assert_eq!(data.quads_for_object(original.object.as_ref()).count(), 1);
    assert_eq!(data.graph(GraphNameRef::DefaultGraph).len(), 1);
}

#[test]
fn unwinding_rolls_back_without_losing_preexisting_or_duplicate_quads() {
    let original = quad(
        BlankNode::new("original").unwrap().into(),
        GraphName::DefaultGraph,
    );
    let mut data = Dataset::from_iter([original.clone()]);
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut transaction = data.insert_transaction();
        assert!(!transaction.insert(&original));
        transaction.insert(&quad(
            Literal::new_simple_literal("temporary").into(),
            GraphName::DefaultGraph,
        ));
        panic!("inference interrupted");
    }));
    assert!(failure.is_err());
    assert_eq!(data, Dataset::from_iter([original]));
}

#[test]
fn rollback_releases_generated_blank_nodes_and_cloned_snapshots_are_independent() {
    let original = quad(
        Literal::new_simple_literal("original").into(),
        GraphName::DefaultGraph,
    );
    let generated = BlankNode::default();
    let added = Quad::new(
        generated.clone(),
        NamedNode::new("urn:p").unwrap(),
        generated.clone(),
        GraphName::DefaultGraph,
    );
    let mut data = Dataset::from_iter([original.clone()]);
    let mut snapshot = {
        let mut transaction = data.insert_transaction();
        transaction.insert(&added);
        // A snapshot owns its terms and must not retain an active journal.
        (*transaction).clone()
    };
    assert_eq!(data, Dataset::from_iter([original]));
    assert!(
        data.encoded_named_or_blank_node(generated.as_ref())
            .is_none()
    );
    assert!(snapshot.contains(&added));
    let mut independent = snapshot.insert_transaction();
    assert!(!independent.insert(&added));
    independent.commit();
}

#[cfg(feature = "rdf-12")]
#[test]
fn rollback_releases_new_quoted_triples_and_preserves_existing_ones() {
    let quoted = Triple::new(
        NamedNode::new("urn:s").unwrap(),
        NamedNode::new("urn:p").unwrap(),
        NamedNode::new("urn:o").unwrap(),
    );
    let original = quad(quoted.clone().into(), GraphName::DefaultGraph);
    let temporary = Triple::new(
        BlankNode::default(),
        NamedNode::new("urn:p").unwrap(),
        Literal::new_simple_literal("temporary"),
    );
    let mut data = Dataset::from_iter([original.clone()]);
    {
        let mut transaction = data.insert_transaction();
        transaction.insert(&quad(temporary.clone().into(), GraphName::DefaultGraph));
        assert!(!transaction.insert(&original));
    }
    assert_eq!(data, Dataset::from_iter([original]));
    assert!(data.encoded_term(TermRef::Triple(&temporary)).is_none());
    assert!(data.encoded_term(TermRef::Triple(&quoted)).is_some());
}

#[cfg(feature = "rdf-10-string-identity")]
#[test]
fn transaction_keeps_plain_and_explicit_string_identity_distinct() {
    let plain = quad(
        Literal::new_simple_literal("value").into(),
        GraphName::DefaultGraph,
    );
    let typed = quad(
        Literal::new_typed_literal("value", vocab::xsd::STRING).into(),
        GraphName::DefaultGraph,
    );
    let mut data = Dataset::from_iter([plain.clone()]);
    let mut transaction = data.insert_transaction();
    assert!(transaction.insert(&typed));
    assert_eq!(transaction.inserted().count(), 1);
    transaction.commit();
    assert_eq!(data, Dataset::from_iter([plain, typed]));
}
