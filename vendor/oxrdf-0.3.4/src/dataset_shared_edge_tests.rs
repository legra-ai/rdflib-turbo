use super::shared_tests::{assert_patterns, fixture};
use super::*;

#[test]
fn generated_numeric_blank_nodes_work_in_all_ranges_and_rollback() {
    let generated = BlankNode::default();
    let labelled = BlankNode::new_unchecked("labelled");
    let predicate = NamedNode::new_unchecked("urn:p");
    let quads = [
        Quad::new(
            generated.clone(),
            predicate.clone(),
            labelled.clone(),
            generated.clone(),
        ),
        Quad::new(
            labelled.clone(),
            predicate.clone(),
            generated.clone(),
            labelled.clone(),
        ),
        Quad::new(
            generated.clone(),
            predicate.clone(),
            generated.clone(),
            GraphName::DefaultGraph,
        ),
    ];
    let mut data = Dataset::from_iter(quads.iter().map(Quad::as_ref));
    let expected: HashSet<_> = quads.into_iter().collect();
    assert_patterns(&data, &expected);
    {
        let mut tx = data.insert_transaction();
        tx.insert(&Quad::new(
            BlankNode::default(),
            predicate.clone(),
            generated.clone(),
            labelled.clone(),
        ));
    }
    assert_patterns(&data, &expected);
    let removed = expected.iter().next().unwrap().clone();
    assert!(data.remove(&removed));
    let mut remainder = expected;
    remainder.remove(&removed);
    assert_patterns(&data, &remainder);
}

#[cfg(feature = "rdfc-10")]
#[test]
fn clone_owns_decode_state_after_source_drop_and_independent_canonicalization() {
    let initial = Quad::new(
        BlankNode::new_unchecked("original"),
        NamedNode::new_unchecked("urn:p"),
        Literal::new_simple_literal("original lexical"),
        BlankNode::new_unchecked("original-graph"),
    );
    let added = Quad::new(
        BlankNode::default(),
        NamedNode::new_unchecked("urn:added"),
        BlankNode::new_unchecked("added-object"),
        GraphName::DefaultGraph,
    );
    let mut source = Dataset::from_iter([initial.clone()]);
    let mut snapshot = {
        let mut tx = source.insert_transaction();
        tx.insert(&added);
        (*tx).clone()
    };
    assert!(!source.contains(&added));
    {
        let borrowed = snapshot.iter().next().unwrap();
        let expected = borrowed.into_owned();
        source.clear();
        source.insert(&Quad::new(
            NamedNode::new_unchecked("urn:reused"),
            NamedNode::new_unchecked("urn:p"),
            Literal::new_simple_literal("reused lexical"),
            GraphName::DefaultGraph,
        ));
        drop(source);
        assert_eq!(borrowed, expected.as_ref());
    }
    assert!(snapshot.remove(&added));
    let mut independent = snapshot.clone();
    let original_text = independent.to_string();
    let snapshot_only = Quad::new(
        BlankNode::new_unchecked("snapshot-only"),
        NamedNode::new_unchecked("urn:p"),
        Literal::new_simple_literal("snapshot"),
        GraphName::DefaultGraph,
    );
    let independent_only = Quad::new(
        BlankNode::new_unchecked("independent-only"),
        NamedNode::new_unchecked("urn:p"),
        Literal::new_simple_literal("independent"),
        GraphName::DefaultGraph,
    );
    snapshot.insert(&snapshot_only);
    assert_eq!(independent.to_string(), original_text);
    independent.insert(&independent_only);
    assert!(!snapshot.contains(&independent_only));
    assert!(!independent.contains(&snapshot_only));
    let independent_text = independent.to_string();
    snapshot.canonicalize(CanonicalizationAlgorithm::default());
    assert_eq!(independent.to_string(), independent_text);
    let canonical_snapshot = snapshot.clone();
    let snapshot_text = snapshot.to_string();
    independent.canonicalize(CanonicalizationAlgorithm::default());
    assert_eq!(snapshot, canonical_snapshot);
    assert_eq!(snapshot.to_string(), snapshot_text);
}

#[test]
fn independent_record_allocations_and_sentinels_keep_each_exact_order() {
    let mut data = Dataset::from_iter(fixture());
    let mut records: Vec<_> = data
        .spog
        .iter()
        .map(|q| q.as_tuple())
        .take(8)
        .collect();
    records.push((
        InternedNamedOrBlankNode::first(),
        InternedNamedNode::first(),
        InternedTerm::first(),
        InternedGraphName::first(),
    ));
    records.push((
        InternedNamedOrBlankNode::impossible(),
        InternedNamedNode::impossible(),
        InternedTerm::impossible(),
        InternedGraphName::impossible(),
    ));
    for id in [0, 1, u128::MAX - 1, u128::MAX] {
        let b = InternedNamedOrBlankNode::encoded_into(
            BlankNode::new_from_unique_id(id).as_ref().into(),
            &mut data.interner,
        );
        records.push((
            b,
            InternedNamedNode::first(),
            InternedTerm::first(),
            InternedGraphName::first(),
        ));
        records.push((
            b.next(),
            InternedNamedNode::first(),
            InternedTerm::first(),
            InternedGraphName::first(),
        ));
    }
    for fields in &records {
        let first = Arc::new(QuadRecord::from_spog(fields.clone()));
        let second = Arc::new(QuadRecord::from_spog(fields.clone()));
        assert!(!Arc::ptr_eq(&first, &second));
        macro_rules! equal {
            ($key:ident) => {{
                let a = $key::new(first.clone());
                let b = $key::new(second.clone());
                assert_eq!(a, b);
                assert_eq!(a.cmp(&b), std::cmp::Ordering::Equal);
            }};
        }
        equal!(GspoKey);
        equal!(GposKey);
        equal!(GospKey);
        equal!(SpogKey);
        equal!(PosgKey);
        equal!(OspgKey);
        let key = GspoKey::new(first);
        let borrowed: &QuadRecord = std::borrow::Borrow::borrow(&key);
        assert_eq!(borrowed, second.as_ref());
        assert_eq!(
            borrowed.cmp(&second),
            key.cmp(&GspoKey::new(second.clone()))
        );
    }
    macro_rules! order {
        ($key:ident,($a:tt,$b:tt,$c:tt,$d:tt)) => {{
            for a in &records {
                for b in &records {
                    let ka = $key::new(Arc::new(QuadRecord::from_spog(a.clone())));
                    let kb = $key::new(Arc::new(QuadRecord::from_spog(b.clone())));
                    assert_eq!(
                        ka.cmp(&kb),
                        (&a.$a, &a.$b, &a.$c, &a.$d).cmp(&(&b.$a, &b.$b, &b.$c, &b.$d))
                    );
                }
            }
        }};
    }
    order!(GspoKey, (3, 0, 1, 2));
    order!(GposKey, (3, 1, 2, 0));
    order!(GospKey, (3, 2, 0, 1));
    order!(SpogKey, (0, 1, 2, 3));
    order!(PosgKey, (1, 2, 0, 3));
    order!(OspgKey, (2, 0, 1, 3));
}
