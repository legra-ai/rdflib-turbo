//! Same-relation generalized storage, transactions and owned snapshot contracts.

use oxigraph::model::{
    BlankNode, GeneralizedQuad, GeneralizedTermPosition, GraphName, Literal, NamedNode, Quad, Term,
};
use oxigraph::store::{StorageError, Store};
use std::collections::HashSet;

fn iri(value: &str) -> NamedNode {
    NamedNode::new(value).unwrap()
}

fn fixture() -> Vec<GeneralizedQuad> {
    // bounded: 3 term kinds x 3 term kinds x 3 graph names, synthetic regression data.
    let terms = [
        Term::from(iri("urn:term")),
        BlankNode::new("term").unwrap().into(),
        Literal::new_typed_literal(
            "a long exact lexical value 0001",
            iri("urn:datatype:custom:long"),
        )
        .into(),
    ];
    let graphs = [
        GraphName::DefaultGraph,
        iri("urn:graph").into(),
        BlankNode::new("graph").unwrap().into(),
    ];
    let mut quads = Vec::new();
    for (subject_index, subject) in terms.iter().enumerate() {
        for (predicate_index, predicate) in terms.iter().enumerate() {
            for graph in &graphs {
                quads.push(GeneralizedQuad::new(
                    subject.clone(),
                    predicate.clone(),
                    terms[(subject_index + predicate_index) % terms.len()].clone(),
                    graph.clone(),
                ));
            }
        }
    }
    quads
}

#[test]
fn generalized_relation_preserves_all_sixteen_index_masks() {
    let store = Store::new().unwrap();
    let quads = fixture();
    for quad in &quads {
        store.insert_generalized(quad).unwrap();
        store.insert_generalized(quad).unwrap();
    }
    assert_eq!(store.len().unwrap(), quads.len());
    for probe in &quads {
        for mask in 0..16 {
            let subject = (mask & 1 != 0).then(|| probe.subject.as_ref());
            let predicate = (mask & 2 != 0).then(|| probe.predicate.as_ref());
            let object = (mask & 4 != 0).then(|| probe.object.as_ref());
            let graph = (mask & 8 != 0).then(|| probe.graph_name.as_ref());
            // bounded: at most 27 synthetic fixture rows in either result set.
            let actual: HashSet<_> = store
                .generalized_quads_for_pattern(subject, predicate, object, graph)
                .map(Result::unwrap)
                .collect();
            let expected: HashSet<_> = quads
                .iter()
                .filter(|quad| {
                    subject.is_none_or(|term| term == quad.subject.as_ref())
                        && predicate.is_none_or(|term| term == quad.predicate.as_ref())
                        && object.is_none_or(|term| term == quad.object.as_ref())
                        && graph.is_none_or(|term| term == quad.graph_name.as_ref())
                })
                .cloned()
                .collect();
            assert_eq!(actual, expected, "mask={mask}, probe={probe:?}");
        }
    }
    assert_eq!(
        store
            .generalized_quads_for_pattern(
                Some(iri("urn:absent").as_ref().into()),
                None,
                None,
                None
            )
            .count(),
        0
    );
    store.validate().unwrap();
}

#[test]
fn generalized_transactions_read_pending_state_and_rollback_without_losing_dictionary() {
    let store = Store::new().unwrap();
    let quads = fixture();
    store.insert_generalized(&quads[0]).unwrap();
    {
        let mut transaction = store.start_transaction().unwrap();
        transaction.insert_generalized(&quads[26]);
        transaction.remove_generalized(&quads[0]);
        // bounded: exactly one pending fixture row.
        let pending: Vec<_> = transaction
            .generalized_quads_for_pattern(None, None, None, None)
            .map(Result::unwrap)
            .collect();
        assert_eq!(pending, [quads[26].clone()]);
        // Dropping an uncommitted transaction rolls back both changes.
    }
    assert_eq!(store.len().unwrap(), 1);
    assert_eq!(
        store
            .generalized_quads_for_pattern(None, None, None, None)
            .next()
            .unwrap()
            .unwrap(),
        quads[0]
    );
    let mut transaction = store.start_transaction().unwrap();
    transaction.insert_generalized(&quads[26]);
    transaction.remove_generalized(&quads[0]);
    transaction.commit().unwrap();
    store.remove_generalized(&quads[26]).unwrap();
    assert!(store.is_empty().unwrap());
    store.validate().unwrap();
}

#[test]
fn generalized_owned_snapshot_keeps_version_and_lexical_owner_after_source_drop() {
    let store = Store::new().unwrap();
    let quad = fixture().pop().unwrap();
    store.insert_generalized(&quad).unwrap();
    let snapshot = store.read_snapshot();
    let mut rows = snapshot.generalized_quads_for_pattern(None, None, None, None);
    drop(snapshot);
    store.remove_generalized(&quad).unwrap();
    drop(store);
    std::thread::spawn(move || {
        assert_eq!(rows.next().unwrap().unwrap(), quad);
        assert!(rows.next().is_none());
    })
    .join()
    .unwrap();
}

#[test]
fn strict_traversal_errors_instead_of_filtering_generalized_rows() {
    let store = Store::new().unwrap();
    let literal = Literal::new_simple_literal("subject");
    store
        .insert_generalized(&GeneralizedQuad::new(
            literal,
            iri("urn:p"),
            iri("urn:o"),
            GraphName::DefaultGraph,
        ))
        .unwrap();
    store
        .insert_generalized(&GeneralizedQuad::new(
            iri("urn:s"),
            BlankNode::new("predicate").unwrap(),
            iri("urn:o"),
            GraphName::DefaultGraph,
        ))
        .unwrap();
    let strict = Quad::new(
        iri("urn:s"),
        iri("urn:p"),
        iri("urn:o"),
        GraphName::DefaultGraph,
    );
    store.insert(&strict).unwrap();
    let mut subject_errors = 0;
    let mut predicate_errors = 0;
    let mut successes = 0;
    for row in &store {
        match row {
            Ok(quad) => {
                assert_eq!(quad, strict);
                successes += 1;
            }
            Err(StorageError::GeneralizedNarrowing(error)) => match error.position() {
                GeneralizedTermPosition::Subject => subject_errors += 1,
                GeneralizedTermPosition::Predicate => predicate_errors += 1,
            },
            Err(error) => panic!("unexpected strict narrowing error: {error}"),
        }
    }
    assert_eq!((successes, subject_errors, predicate_errors), (1, 1, 1));
    store.validate().unwrap();
}

#[test]
fn generalized_storage_keeps_exact_literal_identity_in_subject_and_predicate() {
    let store = Store::new().unwrap();
    // bounded: six lexical identity controls, inserted in each generalized slot.
    let terms = [
        Literal::new_typed_literal("01", iri("http://www.w3.org/2001/XMLSchema#integer")),
        Literal::new_typed_literal("1", iri("http://www.w3.org/2001/XMLSchema#integer")),
        Literal::new_simple_literal("text"),
        Literal::new_typed_literal("text", iri("http://www.w3.org/2001/XMLSchema#string")),
        Literal::new_language_tagged_literal("text", "en").unwrap(),
        Literal::new_language_tagged_literal("text", "nl").unwrap(),
    ];
    for term in terms {
        let quad = GeneralizedQuad::new(term.clone(), term, iri("urn:o"), GraphName::DefaultGraph);
        store.insert_generalized(&quad).unwrap();
        assert_eq!(
            store
                .generalized_quads_for_pattern(
                    Some(quad.subject.as_ref()),
                    Some(quad.predicate.as_ref()),
                    None,
                    None,
                )
                .next()
                .unwrap()
                .unwrap(),
            quad
        );
    }
    assert_eq!(store.len().unwrap(), 6);
    store.validate().unwrap();
}
