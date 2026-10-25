use super::*;

#[test]
fn store_snapshots_and_direct_quads_restore_only_plain_string_identity() {
    let relative = parse_dataset(
        "<s> <p> <o>.",
        RdfFormat::Turtle,
        Some("https://example.org/"),
    )
    .unwrap();
    assert_eq!(
        relative.iter().next().unwrap().subject.to_string(),
        "<https://example.org/s>"
    );
    let source = "<urn:s> <urn:p> \"plain\", \"typed\"^^<http://www.w3.org/2001/XMLSchema#string>, \"français\"@fr, 1.";
    let data = parse_dataset(source, RdfFormat::Turtle, None).unwrap();
    let serialized = serialize(&data).unwrap();
    let restored = parse_store_snapshot(&serialized).unwrap();
    for quad in restored.iter() {
        if let TermRef::Literal(literal) = quad.object {
            if literal.datatype() == iri(XSD, "string").as_ref() {
                assert!(literal.has_explicit_datatype());
            } else if literal.language().is_some() {
                assert_eq!(literal.language(), Some("fr"));
            }
        }
        assert!(restored.contains(store_quad(quad.into_owned()).as_ref()));
    }
    assert_eq!(restored.len(), data.len());
    assert!(parse("", RdfFormat::Turtle, Some("invalid base")).is_err());
    assert!(parse("bad RDF", RdfFormat::Turtle, None).is_err());
}

#[test]
fn scalar_graph_helpers_reject_ambiguous_and_ill_typed_declarations() {
    let node: Term = iri("urn:", "s").into();
    let literal: Term = Literal::new_simple_literal("literal").into();
    let mut data = Dataset::new();
    assert!(subject(&literal).is_err());
    assert!(
        objects(&data, &literal, &iri(SH, "closed"))
            .unwrap()
            .is_empty()
    );
    assert!(!boolean(&data, &node, "closed").unwrap());
    for (text, expected) in [("true", true), ("1", true), ("false", false), ("0", false)] {
        data.clear();
        add(
            &mut data,
            &node,
            iri(SH, "closed"),
            Literal::new_typed_literal(text, iri(XSD, "boolean")).into(),
        )
        .unwrap();
        assert_eq!(boolean(&data, &node, "closed").unwrap(), expected);
    }
    for term in [
        literal.clone(),
        iri("urn:", "notBoolean").into(),
        Literal::new_typed_literal("bad", iri(XSD, "boolean")).into(),
    ] {
        data.clear();
        add(&mut data, &node, iri(SH, "closed"), term).unwrap();
        assert!(boolean(&data, &node, "closed").is_err());
    }
    add(
        &mut data,
        &node,
        iri(SH, "closed"),
        Literal::from(true).into(),
    )
    .unwrap();
    assert!(
        one(&data, &node, "closed")
            .unwrap_err()
            .to_string()
            .contains("multiple")
    );
    assert!(add(&mut data, &literal, iri(SH, "closed"), literal.clone()).is_err());
}

#[test]
fn cyclic_blank_descriptions_keep_root_and_copy_to_reference_depth() {
    let a: Term = BlankNode::new("a").unwrap().into();
    let b: Term = BlankNode::new("b").unwrap().into();
    let mut source = Dataset::new();
    add(&mut source, &a, iri("urn:", "p"), b.clone()).unwrap();
    add(&mut source, &b, iri("urn:", "p"), a.clone()).unwrap();
    let mut report = Dataset::new();
    let mut clones = ReportClones::default();
    clones.copy(&source, &mut report, &a).unwrap();
    clones.copy(&source, &mut report, &a).unwrap();
    clones
        .copy(&source, &mut report, &iri("urn:", "named").into())
        .unwrap();
    assert_eq!(report.len(), 5);
    assert_eq!(objects(&report, &a, &iri("urn:", "p")).unwrap().len(), 1);
    let display = crate::display::Display {
        shape_namespaces: vec![],
        data_namespaces: vec![],
        data: &source,
        shapes: &source,
    };
    assert_eq!(
        display.term(&a, false).unwrap(),
        format!(
            "{}<http://recursion.too.deep>{}",
            "[ <urn:p> ".repeat(6),
            " ]".repeat(6)
        )
    );
}

#[test]
fn subclass_cycles_do_not_duplicate_instances_and_named_contexts_remain_separate() {
    let mut data = Dataset::new();
    let a: Term = iri("urn:", "A").into();
    let b: Term = BlankNode::new("class").unwrap().into();
    let node: Term = iri("urn:", "node").into();
    add(&mut data, &a, iri(RDFS, "subClassOf"), b.clone()).unwrap();
    add(&mut data, &b, iri(RDFS, "subClassOf"), a.clone()).unwrap();
    add(&mut data, &node, iri(RDF, "type"), b).unwrap();
    assert_eq!(instances(&data, &a).unwrap(), HashSet::from([node]));
    data.insert(QuadRef::new(
        NamedNodeRef::new_unchecked("urn:other"),
        iri(RDF, "type").as_ref(),
        iri("urn:", "A").as_ref(),
        NamedNodeRef::new_unchecked("urn:g"),
    ));
    assert_eq!(subjects(&data, &iri(RDF, "type"), None).unwrap().len(), 1);
    assert_eq!(with_union(&data).len(), data.len() + 1);
}

#[test]
fn deeply_nested_blank_descriptions_stop_at_the_display_depth_limit() {
    let mut data = Dataset::new();
    let nodes: Vec<Term> = (0..24)
        .map(|i| BlankNode::new(format!("b{i}")).unwrap().into())
        .collect();
    for pair in nodes.windows(2) {
        add(&mut data, &pair[0], iri("urn:", "p"), pair[1].clone()).unwrap();
    }
    let display = crate::display::Display {
        shape_namespaces: vec![],
        data_namespaces: vec![],
        data: &data,
        shapes: &data,
    };
    let rendered = display.term(&nodes[0], false).unwrap();
    assert!(rendered.contains("<http://recursion.too.deep>"));
    assert_eq!(rendered.matches("<urn:p>").count(), 6);
}

#[test]
fn malformed_lists_and_deep_paths_fail_before_evaluation() {
    for text in [
        "_:a <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <urn:p>; <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> _:a.",
        "_:a <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <urn:p>.",
    ] {
        let data = parse(text, RdfFormat::Turtle, None).unwrap();
        let head: Term = data.iter().next().unwrap().subject.into_owned().into();
        assert!(crate::path::rdf_list(&data, &head).is_err());
    }
    assert!(crate::path::rdf_list(&Dataset::new(), &Literal::from(1).into()).is_err());
    let mut data = Dataset::new();
    let nodes: Vec<Term> = (0..258)
        .map(|i| BlankNode::new(format!("b{i}")).unwrap().into())
        .collect();
    for pair in nodes.windows(2) {
        add(&mut data, &pair[0], iri(SH, "inversePath"), pair[1].clone()).unwrap();
    }
    assert!(
        crate::path::Path::read(&data, &nodes[0])
            .err()
            .unwrap()
            .to_string()
            .contains("nesting limit")
    );
}

#[test]
fn report_descendants_are_fresh_per_occurrence_but_roots_are_cached() {
    let root: Term = BlankNode::new("root").unwrap().into();
    let child: Term = BlankNode::new("child").unwrap().into();
    let mut source = Dataset::new();
    add(&mut source, &root, iri("urn:", "p"), child.clone()).unwrap();
    add(&mut source, &root, iri("urn:", "q"), child.clone()).unwrap();
    add(
        &mut source,
        &child,
        iri("urn:", "leaf"),
        Literal::from("value").into(),
    )
    .unwrap();
    let mut report = Dataset::new();
    let mut clones = ReportClones::default();
    clones.copy(&source, &mut report, &root).unwrap();
    let p = objects(&report, &root, &iri("urn:", "p")).unwrap()[0].clone();
    let q = objects(&report, &root, &iri("urn:", "q")).unwrap()[0].clone();
    assert_ne!(p, q);
    assert_ne!(p, child);
    assert_ne!(q, child);
    assert_eq!(report.len(), 4);
    clones.copy(&source, &mut report, &root).unwrap();
    assert_eq!(report.len(), 4);
}

#[test]
fn repeated_scoped_description_triples_do_not_duplicate_descendant_occurrences() {
    let root: Term = BlankNode::new("root").unwrap().into();
    let child: Term = BlankNode::new("child").unwrap().into();
    let mut source = Dataset::new();
    for graph in ["a", "b"] {
        source.insert(&Quad::new(
            subject(&root).unwrap(),
            iri("urn:", "p"),
            child.clone(),
            iri("urn:", graph),
        ));
    }
    add(
        &mut source,
        &child,
        iri("urn:", "leaf"),
        Literal::from("value").into(),
    )
    .unwrap();
    let mut report = Dataset::new();
    ReportClones::default()
        .copy(&source, &mut report, &root)
        .unwrap();
    assert_eq!(objects(&report, &root, &iri("urn:", "p")).unwrap().len(), 1);
    assert_eq!(report.len(), 2);
}
