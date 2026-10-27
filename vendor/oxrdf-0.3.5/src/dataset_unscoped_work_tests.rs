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
