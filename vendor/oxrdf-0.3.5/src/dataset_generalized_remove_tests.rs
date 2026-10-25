use super::*;
use std::panic::{AssertUnwindSafe, catch_unwind};

fn fixture() -> GeneralizedDataset {
    let mut data = GeneralizedDataset::new();
    let roles: [Term; 3] = [
        NamedNode::new_unchecked("urn:term").into(),
        BlankNode::new_unchecked("term").into(),
        Literal::new_simple_literal("literal").into(),
    ];
    for subject in &roles {
        for predicate in &roles {
            for graph in [
                GraphName::DefaultGraph,
                NamedNode::new_unchecked("urn:scope").into(),
                BlankNode::new_unchecked("urn:scope").into(),
            ] {
                data.insert(&GeneralizedQuad::new(
                    subject.clone(),
                    predicate.clone(),
                    Literal::new_simple_literal("object\0é"),
                    graph,
                ));
            }
        }
    }
    data
}

fn records(data: &GeneralizedDataset) -> Vec<Arc<QuadRecord>> {
    // bounded: exactly27 fixed synthetic records, retained only to prove identity.
    data.data
        .gspo
        .iter()
        .map(|key| Arc::clone(&key.record))
        .collect()
}

fn original_records(data: &GeneralizedDataset, records: &[Arc<QuadRecord>]) {
    for record in records {
        assert!(Arc::ptr_eq(
            &data.data.gspo.get(record.as_ref()).unwrap().record,
            record
        ));
    }
}

#[test]
fn generalized_streaming_removal_preserves_first_arcs_and_current_insertions_on_rollback() {
    let mut data = fixture();
    // bounded: fixed27-row synthetic rollback oracle.
    let original = data.clone();
    let original_arcs = records(&data);
    {
        let mut transaction = data.transaction();
        let fresh = GeneralizedQuad::new(
            Literal::new_simple_literal("fresh-subject"),
            BlankNode::new_unchecked("fresh-predicate"),
            Literal::new_simple_literal("fresh-object"),
            GraphName::DefaultGraph,
        );
        transaction.insert(&fresh);
        assert_eq!(transaction.inserted().size_hint().0, 1);
        assert_eq!(
            transaction.remove_where(|quad| quad.predicate.is_blank_node()),
            10
        );
        assert_eq!(
            transaction.inserted().count(),
            0,
            "new then removed is not an addition"
        );
        assert!(
            transaction
                .iter()
                .all(|quad| !quad.predicate.is_blank_node())
        );
    }
    assert_eq!(data, original);
    original_records(&data, &original_arcs);
}

#[test]
fn generalized_streaming_removal_keeps_exact_graph_kinds_and_all_pattern_masks() {
    for graph in [
        GraphName::DefaultGraph,
        NamedNode::new_unchecked("urn:scope").into(),
        BlankNode::new_unchecked("urn:scope").into(),
    ] {
        let mut data = fixture();
        // bounded: fixed27-row synthetic exact semantic oracle.
        let expected = GeneralizedDataset::from_iter(
            data.iter()
                .filter(|q| !(q.predicate.is_blank_node() && q.graph_name == graph.as_ref())),
        );
        let mut transaction = data.transaction();
        assert_eq!(
            transaction.remove_where(
                |quad| quad.predicate.is_blank_node() && quad.graph_name == graph.as_ref()
            ),
            3
        );
        transaction.commit();
        assert_eq!(data, expected);
        let probe = GeneralizedQuad::new(
            Literal::new_simple_literal("literal"),
            BlankNode::new_unchecked("term"),
            Literal::new_simple_literal("object\0é"),
            graph,
        );
        for mask in 0..16 {
            let s = (mask & 1 != 0).then_some(probe.subject.as_ref());
            let p = (mask & 2 != 0).then_some(probe.predicate.as_ref());
            let o = (mask & 4 != 0).then_some(probe.object.as_ref());
            let g = (mask & 8 != 0).then_some(probe.graph_name.as_ref());
            let actual: Vec<_> = data
                .quads_for_pattern(s, p, o, g)
                .map(|q| q.into_owned())
                .collect();
            let expected: Vec<_> = expected
                .quads_for_pattern(s, p, o, g)
                .map(|q| q.into_owned())
                .collect();
            assert_eq!(actual, expected, "mask={mask}");
        }
    }
}

#[test]
fn generalized_streaming_removal_absence_keeps_additive_fast_path_and_unwind_restores_words() {
    let mut data = fixture();
    {
        let mut transaction = data.transaction();
        let fresh = GeneralizedQuad::new(
            NamedNode::new_unchecked("urn:new"),
            NamedNode::new_unchecked("urn:p"),
            Literal::new_simple_literal("new-word"),
            GraphName::DefaultGraph,
        );
        transaction.insert(&fresh);
        assert_eq!(transaction.remove_where(|_| false), 0);
        assert_eq!(
            transaction.inserted().size_hint().0,
            1,
            "no match must retain direct additive iteration"
        );
    }
    let original = data.clone();
    let original_arcs = records(&data);
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let mut transaction = data.transaction();
            transaction.insert(&GeneralizedQuad::new(
                NamedNode::new_unchecked("urn:new-word"),
                NamedNode::new_unchecked("urn:new-predicate"),
                Literal::new_simple_literal("new-object"),
                GraphName::DefaultGraph,
            ));
            let mut selected = 0;
            transaction.remove_where(|quad| {
                if quad.predicate.is_blank_node() {
                    selected += 1;
                    assert_ne!(selected, 2, "synthetic failure after a completed removal");
                    true
                } else {
                    false
                }
            });
        }))
        .is_err()
    );
    assert_eq!(data, original);
    original_records(&data, &original_arcs);
}
