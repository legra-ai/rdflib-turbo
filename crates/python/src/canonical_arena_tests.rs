use super::*;

fn quad<'a>(terms: [TermRef<'a>; 3], graph_name: GraphNameRef<'a>) -> GeneralizedQuadRef<'a> {
    GeneralizedQuadRef {
        subject: terms[0],
        predicate: terms[1],
        object: terms[2],
        graph_name,
    }
}

#[test]
fn exact_full_term_tags_and_graph_positions_roundtrip() {
    let iri = NamedNode::new_unchecked("");
    let blank = BlankNode::new_unchecked("");
    let values = [
        Literal::new_simple_literal(""),
        Literal::new_simple_literal("nul\0 newline\n漢字🙂"),
        Literal::new_typed_literal("value", NamedNode::new_unchecked("urn:datatype")),
        Literal::new_simple_literal("value"),
        Literal::new_typed_literal(
            "value",
            NamedNode::new_unchecked("http://www.w3.org/2001/XMLSchema#string"),
        ),
        Literal::new_language_tagged_literal_unchecked("bonjour", "fr"),
    ];
    let mut builder = Builder::default();
    let mut expected = Vec::new();
    for value in &values {
        for graph in [
            GraphNameRef::DefaultGraph,
            iri.as_ref().into(),
            blank.as_ref().into(),
        ] {
            for terms in [
                [
                    value.as_ref().into(),
                    blank.as_ref().into(),
                    iri.as_ref().into(),
                ],
                [
                    iri.as_ref().into(),
                    value.as_ref().into(),
                    blank.as_ref().into(),
                ],
                [
                    blank.as_ref().into(),
                    iri.as_ref().into(),
                    value.as_ref().into(),
                ],
            ] {
                let value = quad(terms, graph);
                builder.push_quad(value);
                expected.push(Record::Quad(value));
            }
        }
    }
    for graph in [iri.as_ref().into(), blank.as_ref().into()] {
        builder.push_graph(graph);
        expected.push(Record::NamedGraph(graph));
    }
    let records = builder.finish();
    assert_eq!(records.len(), expected.len());
    for actual in records.iter() {
        assert!(expected.contains(&actual), "unexpected record {actual:?}");
    }
}

#[test]
fn graph_declarations_and_quad_placement_have_distinct_keys() {
    let iri = NamedNodeRef::new_unchecked("urn:g");
    let blank = BlankNodeRef::new_unchecked("urn:g");
    let value = LiteralRef::new_simple_literal("");
    let terms = [value.into(), value.into(), value.into()];
    let mut keys = Vec::new();
    for graph in [GraphNameRef::DefaultGraph, iri.into(), blank.into()] {
        let mut builder = Builder::default();
        builder.push_quad(quad(terms, graph));
        keys.push(builder.finish());
    }
    for graph in [iri.into(), blank.into()] {
        let mut builder = Builder::default();
        builder.push_graph(graph);
        keys.push(builder.finish());
    }
    for (index, key) in keys.iter().enumerate() {
        assert!(keys[index + 1..].iter().all(|other| key != other));
    }
}

#[test]
fn batch_order_and_duplicate_witnesses_do_not_survive_normalization() {
    let values = (0..257)
        .map(|i| Literal::new_simple_literal(format!("漢字🙂{i}")))
        .collect::<Vec<_>>();
    let iri = NamedNodeRef::new_unchecked("urn:shape");
    let mut one = Builder::default();
    let mut many = Builder::default();
    for value in &values {
        let row = quad(
            [iri.into(), iri.into(), value.as_ref().into()],
            GraphNameRef::DefaultGraph,
        );
        one.push_quad(row);
    }
    one.push_graph(iri.into());
    for value in values.iter().rev().chain(values.iter()) {
        many.push_quad(quad(
            [iri.into(), iri.into(), value.as_ref().into()],
            GraphNameRef::DefaultGraph,
        ));
        many.push_graph(iri.into());
        many.finish_batch();
    }
    let one = one.finish();
    let many = many.finish();
    assert!(one == many);
    assert_eq!(one.len(), 258);
    assert_eq!(one.retained_bytes(), many.retained_bytes());
    assert_eq!(
        std::mem::size_of::<Span>(),
        2 * std::mem::size_of::<usize>()
    );
}

#[test]
fn field_boundaries_and_record_length_thresholds_preserve_all_payload() {
    let empty = LiteralRef::new_simple_literal("");
    let values = [
        0usize, 1, 119, 120, 121, 122, 123, 127, 128, 16375, 16376, 16377, 16383, 16384,
    ]
    .map(|n| Literal::new_simple_literal("x".repeat(n)));
    let mut builder = Builder::default();
    for value in values.iter().rev().chain(values.iter()) {
        builder.push_quad(quad(
            [value.as_ref().into(), empty.into(), empty.into()],
            GraphNameRef::DefaultGraph,
        ));
    }
    let records = builder.finish();
    assert_eq!(records.len(), values.len());
    assert_eq!(
        records.retained_bytes(),
        values
            .iter()
            .map(|v| v.value().len()
                + 8
                + usize::from(v.value().len() >= 128)
                + usize::from(v.value().len() >= 16384))
            .sum::<usize>()
    );
    for row in records.iter() {
        let Record::Quad(row) = row else {
            panic!("expected quad")
        };
        assert!(values.iter().any(|v| row.subject == v.as_ref().into()));
    }
    let literals = [
        Literal::new_simple_literal("ab"),
        Literal::new_simple_literal("c"),
        Literal::new_simple_literal("a"),
        Literal::new_simple_literal("bc"),
    ];
    let mut first = Builder::default();
    let mut second = Builder::default();
    first.push_quad(quad(
        [
            literals[0].as_ref().into(),
            literals[1].as_ref().into(),
            empty.into(),
        ],
        GraphNameRef::DefaultGraph,
    ));
    second.push_quad(quad(
        [
            literals[2].as_ref().into(),
            literals[3].as_ref().into(),
            empty.into(),
        ],
        GraphNameRef::DefaultGraph,
    ));
    assert!(first.finish() != second.finish());
}

#[test]
fn temporary_frames_do_not_change_canonical_record_order() {
    let short = NamedNode::new_unchecked("urn:s");
    let long = NamedNode::new_unchecked(format!("urn:{}", "x".repeat(124)));
    let predicate = NamedNodeRef::new_unchecked("urn:p");
    let long_object = Literal::new_simple_literal("y".repeat(128));
    let short_object = LiteralRef::new_simple_literal("x");
    let mut builder = Builder::default();
    // Frame lengths would put the long-subject record first. Compare only typed
    // payloads so the short subject sorts first independently of its object.
    builder.push_quad(quad(
        [long.as_ref().into(), predicate.into(), short_object.into()],
        GraphNameRef::DefaultGraph,
    ));
    builder.push_quad(quad(
        [
            short.as_ref().into(),
            predicate.into(),
            long_object.as_ref().into(),
        ],
        GraphNameRef::DefaultGraph,
    ));
    let records = builder.finish();
    let Record::Quad(first) = records.iter().next().unwrap() else {
        panic!("expected quad")
    };
    assert_eq!(first.subject, short.as_ref().into());
    // Original 527 triple payload286 plus explicit quad/default tags per row.
    assert_eq!(records.retained_bytes(), 290);
}
