use super::*;

#[test]
fn floating_message_corpus_matches_original_python_values() {
    let graph = Dataset::new();
    let display = Display {
        shape_namespaces: Vec::new(),
        data_namespaces: Vec::new(),
        data: &graph,
        shapes: &graph,
    };
    for row in include_str!("../tests/fixtures/python_float_messages.tsv").lines() {
        if row.starts_with('#') {
            continue;
        }
        let mut fields = row.split('\t');
        let bits = u64::from_str_radix(fields.next().unwrap(), 16).unwrap();
        let lexical = fields.next().unwrap();
        let expected = fields.next().unwrap();
        assert!(fields.next().is_none());
        assert_eq!(lexical.parse::<f64>().unwrap().to_bits(), bits);
        for datatype in ["float", "double"] {
            let term = Literal::new_typed_literal(lexical, iri(XSD, datatype)).into();
            assert_eq!(display.message(&term), expected, "{bits:016x} {datatype}");
        }
    }
}

#[test]
fn programmatic_names_use_the_original_n3_character_rendering_policy() {
    let graph = Dataset::new();
    let display = Display {
        shape_namespaces: vec![("bad".into(), "urn:invalid ".into())],
        data_namespaces: Vec::new(),
        data: &graph,
        shapes: &graph,
    };
    for forbidden in "<>\" {}|\\^`".chars() {
        let lexical = format!("urn:invalid{forbidden}name");
        assert_eq!(display.iri(&NamedNode::new_unchecked(&lexical)), lexical);
    }
    for lexical in [
        "",
        "relative",
        "relative/path",
        "urn:absolute",
        "urn:unicode:漢字",
    ] {
        assert_eq!(
            display.iri(&NamedNode::new_unchecked(lexical)),
            format!("<{lexical}>")
        );
    }
}

#[test]
fn distinct_blank_values_keep_identical_rendered_descriptions() {
    let mut graph = Dataset::new();
    let head: Term = BlankNode::new("head").unwrap().into();
    // bounded: two distinct values test RDF identity rather than rendered text.
    for node in ["left", "right"] {
        add(
            &mut graph,
            &head,
            iri("urn:example:", "note"),
            BlankNode::new(node).unwrap().into(),
        )
        .unwrap();
    }
    let display = Display {
        shape_namespaces: Vec::new(),
        data_namespaces: vec![("ex".into(), "urn:example:".into())],
        data: &graph,
        shapes: &graph,
    };
    assert_eq!(display.term(&head, false).unwrap(), "[ ex:note [ ], [ ] ]");
}

#[test]
fn completed_blank_descriptions_are_reused_on_deeper_shared_paths() {
    let mut graph = Dataset::new();
    // bounded: seven fixed edges reproduce one rendered annotation's shared DAG.
    for (from, predicate, to) in [
        ("wrapper", "note", "head"),
        ("head", "a", "shared"),
        ("head", "z", "chain0"),
        ("chain0", "next", "chain1"),
        ("chain1", "next", "shared"),
        ("shared", "next", "tail0"),
        ("tail0", "next", "tail1"),
    ] {
        add(
            &mut graph,
            &BlankNode::new(from).unwrap().into(),
            iri("urn:example:", predicate),
            BlankNode::new(to).unwrap().into(),
        )
        .unwrap();
    }
    add(
        &mut graph,
        &BlankNode::new("tail1").unwrap().into(),
        iri("urn:example:", "value"),
        Literal::new_simple_literal("end").into(),
    )
    .unwrap();
    let display = Display {
        shape_namespaces: Vec::new(),
        data_namespaces: vec![("ex".into(), "urn:example:".into())],
        data: &graph,
        shapes: &graph,
    };
    assert_eq!(
        display
            .term(&BlankNode::new("wrapper").unwrap().into(), false)
            .unwrap(),
        r#"[ ex:note [ ex:a [ ex:next [ ex:next [ ex:value Literal("end") ] ] ] ; ex:z [ ex:next [ ex:next [ ex:next [ ex:next [ ex:value Literal("end") ] ] ] ] ] ] ]"#
    );
}

fn render(literal: Literal) -> String {
    let graph = Dataset::new();
    let namespaces = vec![
        ("xsd".into(), XSD.into()),
        ("ex".into(), "urn:example:".into()),
    ];
    let display = Display {
        shape_namespaces: namespaces.clone(),
        data_namespaces: namespaces,
        data: &graph,
        shapes: &graph,
    };
    display.term(&literal.into(), false).unwrap()
}

#[test]
fn report_literals_match_pinned_reference_lexical_identity() {
    let cases = [
        (Literal::new_simple_literal("plain"), "Literal(\"plain\")"),
        (
            Literal::new_simple_literal("say \"hi\"\\end"),
            "Literal(\"say \"hi\"\\end\")",
        ),
        (
            Literal::new_simple_literal("line\nnext\tcol\rreturn"),
            "Literal(\"line\nnext\tcol\rreturn\")",
        ),
        (
            Literal::new_language_tagged_literal("français", "fr").unwrap(),
            "Literal(\"français\", lang=fr)",
        ),
        (
            Literal::new_typed_literal("same", iri(XSD, "string")),
            "Literal(\"same\", datatype=xsd:string)",
        ),
        (
            Literal::new_typed_literal("1", iri(XSD, "integer")),
            "Literal(\"1\", datatype=xsd:integer)",
        ),
        (
            Literal::new_typed_literal("+001", iri(XSD, "integer")),
            "Literal(\"+001\" = 1, datatype=xsd:integer)",
        ),
        (
            Literal::new_typed_literal("true", iri(XSD, "boolean")),
            "Literal(\"true\" = True, datatype=xsd:boolean)",
        ),
        (
            Literal::new_typed_literal("1", iri(XSD, "boolean")),
            "Literal(\"1\" = True, datatype=xsd:boolean)",
        ),
        (
            Literal::new_typed_literal("01.20", iri(XSD, "decimal")),
            "Literal(\"01.20\" = 1.20, datatype=xsd:decimal)",
        ),
        (
            Literal::new_typed_literal("opaque", iri("urn:example:", "opaque")),
            "Literal(\"opaque\", datatype=ex:opaque)",
        ),
        (
            Literal::new_typed_literal("bad", iri(XSD, "integer")),
            "Literal(\"bad\", datatype=xsd:integer)",
        ),
    ];
    for (literal, expected) in cases {
        assert_eq!(render(literal.clone()), expected, "{literal}");
    }
}

#[test]
fn collection_descriptions_keep_order_exact_terms_and_selected_context() {
    let graph = parse_dataset(
        "@prefix ex:<urn:example:>. @prefix rdf:<http://www.w3.org/1999/02/22-rdf-syntax-ns#>. \
         @prefix xsd:<http://www.w3.org/2001/XMLSchema#>. \
         ex:g { _:head rdf:first \"z\"; rdf:rest _:tail. \
         _:tail rdf:first _:inner; rdf:rest rdf:nil. \
         _:inner rdf:first \"same\"^^xsd:string; rdf:rest _:last. \
         _:last rdf:first \"français\"@fr; rdf:rest rdf:nil. } \
         ex:other { _:head rdf:first \"WRONG\";rdf:rest rdf:nil. } \
         ex:aaa { _:inner rdf:first \"WRONG\";rdf:rest rdf:nil. }",
        oxigraph::io::RdfFormat::TriG,
        None,
    )
    .unwrap();
    let namespaces = vec![
        ("ex".into(), "urn:example:".into()),
        ("xsd".into(), XSD.into()),
        ("rdf".into(), RDF.into()),
    ];
    let display = Display {
        shape_namespaces: namespaces.clone(),
        data_namespaces: namespaces,
        data: &graph,
        shapes: &graph,
    };
    let head: Term = BlankNode::new("head").unwrap().into();
    assert_eq!(
        display.term(&head, false).unwrap(),
        "( Literal(\"z\") ( Literal(\"same\", datatype=xsd:string) Literal(\"français\", lang=fr) ) )"
    );
}

#[test]
fn malformed_and_cyclic_collection_descriptions_fail_before_partial_output() {
    for source in [
        "_:head <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <urn:p>.",
        "_:head <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <urn:p>; <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> _:head.",
    ] {
        let graph = parse(source, oxigraph::io::RdfFormat::Turtle, None).unwrap();
        let display = Display {
            shape_namespaces: Vec::new(),
            data_namespaces: Vec::new(),
            data: &graph,
            shapes: &graph,
        };
        let head: Term = BlankNode::new("head").unwrap().into();
        assert!(display.term(&head, false).is_err(), "{source}");
    }
}

#[test]
fn collection_descriptions_bound_deep_blank_items_with_reference_sentinel() {
    let mut graph = Dataset::new();
    let head: Term = BlankNode::new("head").unwrap().into();
    // bounded: fixed 23-node regression fixture exceeds the display depth limit.
    let nodes: Vec<Term> = (0..23)
        .map(|index| BlankNode::new(format!("b{index}")).unwrap().into())
        .collect();
    add(&mut graph, &head, iri(RDF, "first"), nodes[0].clone()).unwrap();
    add(&mut graph, &head, iri(RDF, "rest"), iri(RDF, "nil").into()).unwrap();
    for pair in nodes.windows(2) {
        add(&mut graph, &pair[0], iri("urn:", "next"), pair[1].clone()).unwrap();
    }
    let display = Display {
        shape_namespaces: Vec::new(),
        data_namespaces: Vec::new(),
        data: &graph,
        shapes: &graph,
    };
    let text = display.term(&head, false).unwrap();
    assert!(text.contains("<http://recursion.too.deep>"));
    assert!(!text.contains("[ ... ]"));
}

#[test]
fn empty_blank_description_has_one_interior_space() {
    let graph = Dataset::new();
    let display = Display {
        shape_namespaces: Vec::new(),
        data_namespaces: Vec::new(),
        data: &graph,
        shapes: &graph,
    };
    let empty: Term = BlankNode::new("empty").unwrap().into();
    assert_eq!(display.term(&empty, false).unwrap(), "[ ]");
    assert_eq!(display.term(&empty, true).unwrap(), "[ ]");
}

#[test]
fn generic_self_cycle_uses_bounded_reference_description_frames() {
    let graph = parse(
        "_:head <urn:example:back> _:head.",
        oxigraph::io::RdfFormat::Turtle,
        None,
    )
    .unwrap();
    let display = Display {
        shape_namespaces: Vec::new(),
        data_namespaces: vec![("ex".into(), "urn:example:".into())],
        data: &graph,
        shapes: &graph,
    };
    let head: Term = BlankNode::new("head").unwrap().into();
    let expected = format!(
        "{}<http://recursion.too.deep>{}",
        "[ ex:back ".repeat(6),
        " ]".repeat(6)
    );
    assert_eq!(display.term(&head, false).unwrap(), expected);
    assert_eq!(display.term(&head, false).unwrap(), expected);
}

#[test]
fn deep_and_cyclic_blank_descriptions_share_the_reference_frame_limit() {
    // bounded: sixteen pinned-reference cases, each with at most 23 nodes.
    for cyclic in [false, true] {
        for length in [1usize, 2, 5, 6, 7, 8, 10, 23] {
            let mut graph = Dataset::new();
            let nodes: Vec<Term> = (0..length)
                .map(|index| BlankNode::new(format!("n{index}")).unwrap().into())
                .collect();
            for pair in nodes.windows(2) {
                add(&mut graph, &pair[0], iri("urn:", "p"), pair[1].clone()).unwrap();
            }
            if cyclic {
                add(
                    &mut graph,
                    &nodes[length - 1],
                    iri("urn:", "p"),
                    nodes[0].clone(),
                )
                .unwrap();
            }
            let display = Display {
                shape_namespaces: Vec::new(),
                data_namespaces: Vec::new(),
                data: &graph,
                shapes: &graph,
            };
            let (frames, tail) = if cyclic || length > 6 {
                (6, "<http://recursion.too.deep>")
            } else {
                (length - 1, "[ ]")
            };
            let expected = format!(
                "{}{tail}{}",
                "[ <urn:p> ".repeat(frames),
                " ]".repeat(frames)
            );
            assert_eq!(
                display.term(&nodes[0], false).unwrap(),
                expected,
                "cyclic={cyclic}, length={length}"
            );
        }
    }
}

#[test]
fn recursive_collection_items_use_the_reference_description_limit() {
    let graph = parse(
        "_:head <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> _:head; \
         <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> \
         <http://www.w3.org/1999/02/22-rdf-syntax-ns#nil>.",
        oxigraph::io::RdfFormat::Turtle,
        None,
    )
    .unwrap();
    let display = Display {
        shape_namespaces: Vec::new(),
        data_namespaces: Vec::new(),
        data: &graph,
        shapes: &graph,
    };
    let head: Term = BlankNode::new("head").unwrap().into();
    let expected = format!(
        "{}<http://recursion.too.deep>{}",
        "( ".repeat(6),
        " )".repeat(6)
    );
    assert_eq!(display.term(&head, false).unwrap(), expected);
}
