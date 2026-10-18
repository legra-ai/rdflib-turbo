use super::*;
pub(super) fn fixture() -> Vec<Quad> {
    let generated = BlankNode::new_from_unique_id(42);
    let graphs = [
        GraphName::DefaultGraph,
        NamedNode::new_unchecked("urn:g").into(),
        BlankNode::new_unchecked("g").into(),
        generated.clone().into(),
        BlankNode::new_from_unique_id(1).into(),
        BlankNode::new_from_unique_id(u128::MAX - 1).into(),
    ];
    let objects: Vec<Term> = vec![
        NamedNode::new_unchecked("urn:o").into(),
        BlankNode::new_unchecked("object").into(),
        generated.clone().into(),
        Literal::new_simple_literal("value").into(),
        Literal::new_typed_literal("value", vocab::xsd::STRING).into(),
        Literal::new_typed_literal("01", vocab::xsd::INTEGER).into(),
        Literal::new_language_tagged_literal("value", "en")
            .unwrap()
            .into(),
    ];
    #[cfg(feature = "rdf-12")]
    let objects: Vec<Term> = objects
        .into_iter()
        .chain([
            Triple::new(
                BlankNode::new_unchecked("quoted"),
                NamedNode::new_unchecked("urn:quoted"),
                Triple::new(
                    generated.clone(),
                    NamedNode::new_unchecked("urn:nested"),
                    Literal::new_simple_literal("nested"),
                ),
            )
            .into(),
            Literal::new_directional_language_tagged_literal("value", "en", BaseDirection::Ltr)
                .unwrap()
                .into(),
            Literal::new_directional_language_tagged_literal("value", "en", BaseDirection::Rtl)
                .unwrap()
                .into(),
        ])
        .collect();
    let mut result = Vec::new();
    for s in 0..8 {
        for p in 0..3 {
            for o in &objects {
                for g in &graphs {
                    result.push(Quad::new(
                        if s == 7 {
                            generated.clone().into()
                        } else if s == 5 {
                            BlankNode::new_from_unique_id(1).into()
                        } else if s % 2 == 0 {
                            NamedNode::new_unchecked(format!("urn:s{s}")).into()
                        } else {
                            NamedOrBlankNode::from(BlankNode::new_unchecked(format!("s{s}")))
                        },
                        NamedNode::new_unchecked(format!("urn:p{p}")),
                        o.clone(),
                        g.clone(),
                    ));
                }
            }
        }
    }
    result
}
fn observed(data: &Dataset) -> HashSet<Quad> {
    data.iter().map(QuadRef::into_owned).collect()
}
pub(super) fn assert_patterns(data: &Dataset, expected: &HashSet<Quad>) {
    let actual = observed(data);
    assert_eq!(actual.len(), expected.len());
    assert!(actual == *expected, "full exact-quad relation differs");
    let mut targets: Vec<_> = expected.iter().cloned().collect();
    targets.sort_by_cached_key(ToString::to_string);
    let mut selected: Vec<_> = targets.iter().step_by(17).cloned().collect();
    let mut components = HashSet::new();
    for target in targets {
        let distinct = components.insert((0, target.subject.to_string()))
            | components.insert((1, target.object.to_string()))
            | components.insert((2, target.graph_name.to_string()));
        if distinct {
            selected.push(target);
        }
    }
    let mut targets = selected;
    targets.push(Quad::new(
        NamedNode::new_unchecked("urn:absent"),
        NamedNode::new_unchecked("urn:missing"),
        Literal::new_simple_literal("missing"),
        NamedNode::new_unchecked("urn:absent-graph"),
    ));
    #[cfg(feature = "rdf-12")]
    targets.push(Quad::new(
        NamedNode::new_unchecked("urn:absent"),
        NamedNode::new_unchecked("urn:missing"),
        Triple::new(
            BlankNode::new_unchecked("not-present"),
            NamedNode::new_unchecked("urn:nested"),
            Literal::new_simple_literal("absent quoted"),
        ),
        GraphName::DefaultGraph,
    ));
    for target in targets {
        for mask in 0..16 {
            let subject = (mask & 1 != 0).then_some(target.subject.as_ref());
            let predicate = (mask & 2 != 0).then_some(target.predicate.as_ref());
            let object = (mask & 4 != 0).then_some(target.object.as_ref());
            let graph = (mask & 8 != 0).then_some(target.graph_name.as_ref());
            let actual: HashSet<_> = data
                .quads_for_pattern(subject, predicate, object, graph)
                .map(QuadRef::into_owned)
                .collect();
            let wanted: HashSet<_> = expected
                .iter()
                .filter(|q| {
                    subject.is_none_or(|s| s == q.subject.as_ref())
                        && predicate.is_none_or(|p| p == q.predicate.as_ref())
                        && object.is_none_or(|o| o == q.object.as_ref())
                        && graph.is_none_or(|g| g == q.graph_name.as_ref())
                })
                .cloned()
                .collect();
            assert_eq!(actual, wanted, "mask={mask},target={target}");
        }
        let view = data.graph(target.graph_name.as_ref());
        let expected_graph: HashSet<_> = expected
            .iter()
            .filter(|q| q.graph_name == target.graph_name)
            .map(|q| Triple::new(q.subject.clone(), q.predicate.clone(), q.object.clone()))
            .collect();
        assert_eq!(
            view.iter()
                .map(TripleRef::into_owned)
                .collect::<HashSet<_>>(),
            expected_graph
        );
        for mask in 0..8 {
            let subject = (mask & 1 != 0).then_some(target.subject.as_ref());
            let predicate = (mask & 2 != 0).then_some(target.predicate.as_ref());
            let object = (mask & 4 != 0).then_some(target.object.as_ref());
            let actual: HashSet<_> = view
                .triples_for_pattern(subject, predicate, object)
                .map(TripleRef::into_owned)
                .collect();
            let wanted: HashSet<_> = expected_graph
                .iter()
                .filter(|q| {
                    subject.is_none_or(|s| s == q.subject.as_ref())
                        && predicate.is_none_or(|p| p == q.predicate.as_ref())
                        && object.is_none_or(|o| o == q.object.as_ref())
                })
                .cloned()
                .collect();
            assert_eq!(actual, wanted, "graphmask={mask},target={target}");
        }
        assert_eq!(
            view.contains(TripleRef::new(
                &target.subject,
                &target.predicate,
                &target.object
            )),
            expected.contains(&target)
        );
        assert_eq!(data.contains(&target), expected.contains(&target));
    }
}
#[test]
fn all_ranges_and_mutations_match_exact_quad_reference() {
    let mut data = Dataset::new();
    let mut expected = HashSet::new();
    for q in fixture() {
        assert_eq!(data.insert(&q), expected.insert(q.clone()));
        assert!(!data.insert(&q));
    }
    assert_patterns(&data, &expected);
    let snapshot = data.clone();
    let original = expected.clone();
    for q in fixture().into_iter().step_by(3) {
        assert_eq!(data.remove(&q), expected.remove(&q));
        assert!(!data.remove(&q));
    }
    assert_patterns(&data, &expected);
    assert_patterns(&snapshot, &original);
    {
        let mut tx = data.insert_transaction();
        for q in fixture() {
            tx.insert(&q);
        }
        let independent = (*tx).clone();
        assert_patterns(&independent, &original);
    }
    assert_patterns(&data, &expected);
    data.clear();
    assert_patterns(&data, &HashSet::new());
    assert_patterns(&snapshot, &original);
}
#[test]
fn all_six_indexes_and_journal_share_one_immutable_record() {
    let mut data = Dataset::new();
    let q = fixture().pop().unwrap();
    let mut tx = data.insert_transaction();
    assert!(tx.insert(&q));
    assert!(!tx.insert(&q));
    let record = &tx.inserted[0];
    assert_eq!(Arc::strong_count(record), 7);
    assert!(Arc::ptr_eq(record, &tx.gspo.first().unwrap().record));
    assert!(Arc::ptr_eq(record, &tx.gpos.first().unwrap().record));
    assert!(Arc::ptr_eq(record, &tx.gosp.first().unwrap().record));
    assert!(Arc::ptr_eq(record, &tx.spog.first().unwrap().record));
    assert!(Arc::ptr_eq(record, &tx.posg.first().unwrap().record));
    assert!(Arc::ptr_eq(record, &tx.ospg.first().unwrap().record));
    let snapshot = (*tx).clone();
    assert!(Arc::ptr_eq(record, &snapshot.spog.first().unwrap().record));
    assert_eq!(Arc::strong_count(record), 13);
    drop(tx);
    assert_eq!(snapshot.len(), 1);
    assert!(snapshot.contains(&q));
    assert!(data.is_empty());
    assert_eq!(Arc::strong_count(&snapshot.gspo.first().unwrap().record), 6);
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Dataset>();
}
#[test]
fn all_six_orders_match_the_original_tuple_orders() {
    let data = Dataset::from_iter(fixture());
    macro_rules! check {
        ($index:ident,($a:tt,$b:tt,$c:tt,$d:tt)) => {{
            let mut expected: Vec<_> = data
                .spog
                .iter()
                .map(|q| {
                    let p = q.as_tuple();
                    (p.$a.clone(), p.$b.clone(), p.$c.clone(), p.$d.clone())
                })
                .collect();
            expected.sort();
            let actual: Vec<_> = data
                .$index
                .iter()
                .map(|q| {
                    let p = q.as_tuple();
                    (p.$a.clone(), p.$b.clone(), p.$c.clone(), p.$d.clone())
                })
                .collect();
            assert_eq!(actual, expected);
        }};
    }
    check!(gspo, (3, 0, 1, 2));
    check!(gpos, (3, 1, 2, 0));
    check!(gosp, (3, 2, 0, 1));
    check!(spog, (0, 1, 2, 3));
    check!(posg, (1, 2, 0, 3));
    check!(ospg, (2, 0, 1, 3));
}
