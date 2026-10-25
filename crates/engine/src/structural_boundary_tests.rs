use super::*;
use oxigraph::io::RdfFormat;

fn graph(body: &str) -> Dataset {
    crate::parse_graph(
        &format!(
            "@prefix sh: <http://www.w3.org/ns/shacl#>. @prefix ex: <https://example.org/>. {body}"
        ),
        RdfFormat::Turtle,
        None,
    )
    .unwrap()
}

#[test]
fn shape_declarations_fail_fast_for_inconsistent_roles_messages_and_paths() {
    let node = iri("https://example.org/", "S").into();
    for body in [
        "ex:S a sh:NodeShape; sh:path ex:p.",
        "ex:S a sh:PropertyShape.",
        "ex:S sh:message ex:notLiteral.",
        "ex:S sh:path ex:unresolved.",
        "ex:S sh:severity 'not an IRI'.",
    ] {
        assert!(
            read_shape(
                &graph(body),
                &node,
                &HashMap::new(),
                true,
                &Functions::default(),
                &[],
                Default::default(),
            )
            .is_err(),
            "{body}"
        );
    }
    for lexical in [
        "",
        "relative",
        "relative/path",
        "not a standard IRI",
        "urn:absolute",
    ] {
        assert_eq!(
            named_role(NamedNode::new_unchecked(lexical).into(), "shape")
                .unwrap()
                .as_str(),
            lexical,
        );
    }
    for value in [
        Term::from(BlankNode::new("role").unwrap()),
        Literal::new_simple_literal("role").into(),
    ] {
        assert!(named_role(value, "shape").is_err());
    }
}

#[test]
fn validator_and_rule_readers_reject_incomplete_declarations() {
    let node = iri("https://example.org/", "node").into();
    let data = graph("");
    assert!(validator(&data, &node, true).is_err());
    assert!(validator(&data, &node, false).is_err());
    assert!(target_type(&data, &node).is_err());
    for body in [
        "ex:node a sh:SPARQLRule.",
        "ex:node a sh:TripleRule; sh:subject sh:this; sh:predicate ex:p.",
        "ex:node a sh:TripleRule; sh:subject sh:this; sh:object ex:o.",
        "ex:node a sh:TripleRule; sh:predicate ex:p; sh:object ex:o.",
        "ex:node a ex:UnknownRule.",
    ] {
        assert!(
            rule(&graph(body), &node, &Functions::default()).is_err(),
            "{body}"
        );
    }
}

#[test]
fn structural_shape_load_errors_retain_reference_message_and_link() {
    let node = iri("https://example.org/", "S").into();
    for (body, message, link) in [
        (
            "ex:S a sh:NodeShape; sh:path ex:p.",
            "A shape defined as a NodeShape cannot be the subject of a 'sh:path' predicate.",
            "https://www.w3.org/TR/shacl/#node-shapes",
        ),
        (
            "ex:S a sh:PropertyShape.",
            "A shape defined as a PropertyShape must include one `sh:path` property.",
            "https://www.w3.org/TR/shacl/#property-shapes",
        ),
        (
            "ex:S a sh:PropertyShape; sh:path ex:p,ex:q.",
            "A shape defined as a PropertyShape cannot have more than one 'sh:path' property.",
            "https://www.w3.org/TR/shacl/#property-shapes",
        ),
    ] {
        let error = read_shape(
            &graph(body),
            &node,
            &HashMap::new(),
            false,
            &Functions::default(),
            &[],
            Default::default(),
        )
        .err()
        .unwrap();
        assert!(
            matches!(error, Error::ShapeLoad { message: actual, link: actual_link } if actual == message && actual_link == link)
        );
        assert_eq!(
            error.to_string(),
            format!("{message}\nFor reference, see {link}")
        );
    }
}

#[test]
fn literal_path_retains_typed_error_without_reclassifying_other_path_errors() {
    let literal = Literal::new_simple_literal("bad").into();
    let error = crate::path::Path::read(&graph(""), &literal).err().unwrap();
    assert!(matches!(
        error,
        Error::ShapeLoad {
            message: "Values of a property path cannot be a Literal.",
            link: "https://www.w3.org/TR/shacl/#property-paths"
        }
    ));
    let blank = BlankNode::new("missing").unwrap().into();
    assert!(matches!(
        crate::path::Path::read(&graph(""), &blank),
        Err(Error::Invalid(_))
    ));
}
