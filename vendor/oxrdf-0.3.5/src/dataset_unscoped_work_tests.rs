//! Real unscoped pattern work; no clock, production observer or mirrored dataset.
use super::*;

#[test]
fn full_spo_membership_does_not_visit_the_subject_fanout_per_candidate() {
    let subject = NamedNodeRef::new_unchecked("urn:membership:shared-subject");
    let predicate = NamedNodeRef::new_unchecked("urn:membership:shared-predicate");
    let other = NamedNodeRef::new_unchecked("urn:membership:other-subject");
    let absent = Literal::new_simple_literal("known-outside-this-subject");
    let mut visits = [0; 2]; // bounded: exactly the two synthetic fanout sizes.
    for (case, count) in [100, 10_000].into_iter().enumerate() {
        let mut data = Dataset::new();
        data.insert(QuadRef::new(
            other,
            predicate,
            absent.as_ref(),
            GraphNameRef::DefaultGraph,
        ));
        for index in 0..count {
            data.insert(&Quad::new(
                subject,
                predicate,
                Literal::new_simple_literal(format!("value-{index}")),
                GraphName::DefaultGraph,
            ));
        }
        let examined = Cell::new(0);
        for _ in 0..64 {
            let pattern = Pattern::new(
                &data,
                Some(subject.into()),
                Some(predicate),
                Some(absent.as_ref().into()),
                None,
            )
            .unwrap();
            assert_eq!(pattern.iter(&data, Some(&examined)).count(), 0);
        }
        println!(
            "MEMBERSHIP_WORK rows={count} candidates=64 examined={}",
            examined.get()
        );
        visits[case] = examined.get();
    }
    assert!(
        visits.iter().all(|count| *count <= 64),
        "Full-SPO membership decoded unrelated subject fanout: {visits:?}"
    );
}

#[test]
fn full_spo_unscoped_keeps_every_graph_and_temporary_pattern_lifetime() {
    let subject = NamedNodeRef::new_unchecked("urn:membership:subject");
    let predicate = NamedNodeRef::new_unchecked("urn:membership:predicate");
    let object = Literal::new_simple_literal("matched");
    let mut data = Dataset::new();
    for graph in [
        GraphName::DefaultGraph,
        NamedNode::new_unchecked("urn:membership:graph").into(),
        BlankNode::new_unchecked("membership-graph").into(),
    ] {
        data.insert(&Quad::new(subject, predicate, object.clone(), graph));
    }
    for index in 0..100 {
        data.insert(&Quad::new(
            subject,
            predicate,
            Literal::new_simple_literal(format!("noise-{index}")),
            GraphName::DefaultGraph,
        ));
    }
    // bounded: exactly the three physical placements of the selected statement.
    let expected: Vec<_> = data
        .iter()
        .filter(|q| {
            q.subject == subject.into()
                && q.predicate == predicate
                && q.object == object.as_ref().into()
        })
        .collect();
    assert_eq!(expected.len(), 3);
    let actual = {
        let temporary = Literal::new_simple_literal("matched");
        data.quads_for_pattern(
            Some(subject.into()),
            Some(predicate),
            Some(temporary.as_ref().into()),
            None,
        )
    };
    assert_eq!(actual.collect::<Vec<_>>(), expected);
    for quad in &expected {
        assert_eq!(
            data.quads_for_pattern(
                Some(subject.into()),
                Some(predicate),
                Some(object.as_ref().into()),
                Some(quad.graph_name)
            )
            .collect::<Vec<_>>(),
            [*quad]
        );
    }
    let unknown = Literal::new_simple_literal("not-in-the-interner");
    assert!(
        data.quads_for_pattern(
            Some(subject.into()),
            Some(predicate),
            Some(unknown.as_ref().into()),
            None
        )
        .next()
        .is_none()
    );
}

#[test]
fn pair_prefixes_visit_only_matching_physical_rows() {
    let subject = NamedNodeRef::new_unchecked("urn:pair:subject");
    let predicate = NamedNodeRef::new_unchecked("urn:pair:predicate");
    let object = Literal::new_simple_literal("selected");
    let mut all_exact = true;
    for mixed in [false, true] {
        for width in [100, 10_000] {
            let mut data = Dataset::new();
            data.insert(QuadRef::new(subject, predicate, object.as_ref(), GraphNameRef::DefaultGraph));
            if mixed {
                for graph in [GraphName::NamedNode(NamedNode::new_unchecked("urn:pair:named")), GraphName::BlankNode(BlankNode::new_unchecked("pair-blank"))] {
                    data.insert(&Quad::new(subject, predicate, object.clone(), graph));
                }
            }
            for index in 0..width {
                let other_subject = NamedNode::new_unchecked(format!("urn:pair:other-{index}"));
                let other_predicate = NamedNode::new_unchecked(format!("urn:pair:other-predicate-{index}"));
                let other_object = Literal::new_simple_literal(format!("noise-{index}"));
                data.insert(&Quad::new(subject, other_predicate.clone(), other_object.clone(), GraphName::DefaultGraph));
                data.insert(&Quad::new(other_subject, other_predicate, object.clone(), GraphName::DefaultGraph));
            }
            for (role, s, p, o) in [
                ("SP", Some(subject.into()), Some(predicate), None),
                ("SO", Some(subject.into()), None, Some(object.as_ref().into())),
                ("PO", None, Some(predicate), Some(object.as_ref().into())),
            ] {
                let examined = Cell::new(0);
                let actual = Pattern::new(&data, s, p, o, None).unwrap().iter(&data, Some(&examined));
                let count = actual.count();
                let expected = if mixed { 3 } else { 1 };
                assert_eq!(count, expected);
                println!("PAIR_WORK role={role} mixed={mixed} width={width} rows={count} examined={}", examined.get());
                all_exact &= examined.get() == count;
            }
        }
    }
    assert!(all_exact, "pair prefixes examined unrelated physical rows");
}

#[test]
fn pair_prefixes_keep_legacy_order_graphs_and_temporary_pattern_lifetimes() {
    let subject = NamedNodeRef::new_unchecked("urn:pair:subject");
    let predicate = NamedNodeRef::new_unchecked("urn:pair:predicate");
    let object = Literal::new_simple_literal("selected");
    let mut data = Dataset::new();
    for index in 0..8 {
        for graph in [GraphName::DefaultGraph, NamedNode::new_unchecked(format!("urn:pair:graph-{index}")).into(), BlankNode::new_unchecked(format!("pair-{index}")).into()] {
            for (s, p, o) in [
                (subject.into_owned(), predicate.into_owned(), object.clone()),
                (subject.into_owned(), NamedNode::new_unchecked(format!("urn:pair:p-{index}")), object.clone()),
                (NamedNode::new_unchecked(format!("urn:pair:s-{index}")), predicate.into_owned(), object.clone()),
                (subject.into_owned(), predicate.into_owned(), Literal::new_simple_literal(format!("value-{index}"))),
            ] {
                data.insert(&Quad::new(s, p, o, graph.clone()));
            }
        }
    }
    for mask in [3, 5, 6] {
        let s = (mask & 1 != 0).then_some(subject.into());
        let p = (mask & 2 != 0).then_some(predicate);
        let o = (mask & 4 != 0).then_some(object.as_ref().into());
        // bounded: this synthetic dataset has at most 96 physical statements.
        let expected: Vec<_> = if s.is_some() {
            data.quads_for_subject(subject).filter(|q| p.is_none_or(|v| q.predicate == v) && o.is_none_or(|v| q.object == v)).collect()
        } else {
            data.quads_for_object(object.as_ref()).filter(|q| p.is_none_or(|v| q.predicate == v)).collect()
        };
        let actual = {
            let temporary = Literal::new_simple_literal("selected");
            data.quads_for_pattern(s, p, (mask & 4 != 0).then_some(temporary.as_ref().into()), None)
        };
        assert_eq!(actual.collect::<Vec<_>>(), expected, "mask {mask}");
        for unknown_role in [1, 2, 4] {
            if mask & unknown_role == 0 { continue; }
            let unknown_s = NamedNodeRef::new_unchecked("urn:pair:unknown-subject");
            let unknown_p = NamedNodeRef::new_unchecked("urn:pair:unknown-predicate");
            let unknown_o = Literal::new_simple_literal("unknown-object");
            assert!(data.quads_for_pattern(
                if unknown_role == 1 { Some(unknown_s.into()) } else { s },
                if unknown_role == 2 { Some(unknown_p) } else { p },
                if unknown_role == 4 { Some(unknown_o.as_ref().into()) } else { o },
                None,
            ).next().is_none());
        }
    }
}
