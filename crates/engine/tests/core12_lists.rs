//! Execution coverage for the SHACL 1.2 Core 2026-10-14 list draft.

use pyshacl_turbo_engine::oxigraph::{io::RdfFormat, model::*};
use pyshacl_turbo_engine::{CompiledShapes, IndexedShapesSource, Options, parse_graph};

const PREFIX: &str = "@prefix sh:<http://www.w3.org/ns/shacl#>. @prefix rdf:<http://www.w3.org/1999/02/22-rdf-syntax-ns#>. @prefix xsd:<http://www.w3.org/2001/XMLSchema#>.";

fn graph(body: &str) -> Dataset {
    parse_graph(&format!("{PREFIX} {body}"), RdfFormat::Turtle, None).unwrap()
}

fn shapes(body: &str) -> IndexedShapesSource {
    IndexedShapesSource::parse_graph(&format!("{PREFIX} {body}"), RdfFormat::Turtle, None).unwrap()
}

fn program(constraint: &str) -> CompiledShapes {
    CompiledShapes::compile(
        shapes(&format!(
            "<urn:S> sh:targetNode <urn:s>; sh:property [sh:path <urn:p>; {constraint}]. \
         <urn:M> sh:nodeKind sh:IRI."
        )),
        false,
    )
    .unwrap()
}

fn conforms(constraint: &str, data: &str) -> bool {
    program(constraint)
        .validate(&mut graph(data), Options::default())
        .unwrap()
        .conforms
}

#[test]
fn list_lengths_count_members_and_keep_arbitrary_precision_bounds() {
    for (constraint, empty, two) in [
        ("sh:minListLength 0", true, true),
        ("sh:minListLength 2", false, true),
        ("sh:maxListLength 0", true, false),
        ("sh:maxListLength 2", true, true),
        (
            "sh:minListLength 340282366920938463463374607431768211456",
            false,
            false,
        ),
        (
            "sh:maxListLength 340282366920938463463374607431768211456",
            true,
            true,
        ),
    ] {
        assert_eq!(conforms(constraint, "<urn:s> <urn:p> ()."), empty);
        assert_eq!(conforms(constraint, "<urn:s> <urn:p> (1 2)."), two);
    }
}

#[test]
fn all_list_constraints_reject_malformed_structure_including_disabled_uniqueness() {
    for constraint in [
        "sh:minListLength 0",
        "sh:maxListLength 100",
        "sh:uniqueMembers false",
        "sh:memberShape <urn:M>",
    ] {
        for body in [
            "<urn:s> <urn:p> 1.",
            "<urn:s> <urn:p> _:a. _:a rdf:first 1.",
            "<urn:s> <urn:p> _:a. _:a rdf:first 1, 2; rdf:rest rdf:nil.",
            "<urn:s> <urn:p> _:a. _:a rdf:first 1; rdf:rest rdf:nil, _:b.",
            "<urn:s> <urn:p> _:a. _:a rdf:first 1; rdf:rest _:a.",
            "<urn:s> <urn:p> _:a. _:a rdf:first 1; rdf:rest _:b. _:b rdf:first 2; rdf:rest _:a.",
            "<urn:s> <urn:p> _:a. _:a rdf:first 1; rdf:rest 2.",
            "<urn:s> <urn:p> (). rdf:nil rdf:first 1.",
            "<urn:s> <urn:p> (). rdf:nil rdf:rest rdf:nil.",
        ] {
            assert!(!conforms(constraint, body), "{constraint}: {body}");
        }
    }
}

#[test]
fn uniqueness_false_still_checks_lists_and_true_compares_rdf_member_identity() {
    for enabled in [true, false] {
        let constraint = format!("sh:uniqueMembers {enabled}");
        assert!(conforms(&constraint, "<urn:s> <urn:p> ()."));
        assert!(conforms(&constraint, "<urn:s> <urn:p> (1 2 3)."));
        assert_eq!(conforms(&constraint, "<urn:s> <urn:p> (1 2 1)."), !enabled);
        assert_eq!(
            conforms(&constraint, "<urn:s> <urn:p> (_:x _:x)."),
            !enabled
        );
    }
}

#[test]
fn member_shape_reports_the_list_and_nested_member_failures() {
    let report = program("sh:memberShape <urn:M>")
        .validate(
            &mut graph("<urn:s> <urn:p> _:list. _:list rdf:first \"bad\"; rdf:rest rdf:nil."),
            Options::default(),
        )
        .unwrap();
    assert!(!report.conforms);
    let component = NamedNode::new("http://www.w3.org/ns/shacl#sourceConstraintComponent").unwrap();
    let list_component =
        NamedNode::new("http://www.w3.org/ns/shacl#MemberShapeConstraintComponent").unwrap();
    let parent = report
        .graph
        .quads_for_pattern(
            None,
            Some(component.as_ref()),
            Some(list_component.as_ref().into()),
            None,
        )
        .next()
        .unwrap()
        .subject;
    assert!(
        report
            .graph
            .quads_for_pattern(
                Some(parent),
                Some(NamedNodeRef::new("http://www.w3.org/ns/shacl#detail").unwrap()),
                None,
                None
            )
            .next()
            .is_some()
    );
    assert!(
        report
            .graph
            .quads_for_pattern(
                Some(parent),
                Some(NamedNodeRef::new("http://www.w3.org/ns/shacl#value").unwrap()),
                Some(BlankNodeRef::new("list").unwrap().into()),
                None
            )
            .next()
            .is_some()
    );
    assert!(conforms(
        "sh:memberShape <urn:M>",
        "<urn:s> <urn:p> (<urn:a> <urn:b>)."
    ));
}

#[test]
fn list_parameters_reject_negative_noninteger_duplicate_and_property_shape_values() {
    for constraint in [
        "sh:minListLength -1",
        "sh:maxListLength -1",
        "sh:minListLength true",
        "sh:maxListLength 1.5",
        "sh:minListLength 1, 2",
        "sh:uniqueMembers 1",
    ] {
        let shapes = shapes(&format!("<urn:S> sh:targetNode <urn:s>; {constraint}."));
        assert!(
            CompiledShapes::compile(shapes, false).is_err(),
            "{constraint}"
        );
    }
    assert!(
        CompiledShapes::compile(
            shapes(
                "<urn:S> sh:targetNode <urn:s>; sh:memberShape <urn:P>. <urn:P> sh:path <urn:p>."
            ),
            false
        )
        .is_err()
    );
}

#[test]
fn member_shape_rejects_literal_references_at_the_public_parameter_boundary() {
    let result = CompiledShapes::compile(
        shapes(r#"<urn:S> sh:targetNode <urn:s>; sh:memberShape "literal-reference"."#),
        false,
    );
    let error = result.err().expect("literal memberShape must be rejected");
    assert!(
        error
            .to_string()
            .contains("Expected a shape IRI or blank node")
    );
}

#[test]
fn list_length_parameters_require_xsd_integer_lexicals() {
    for name in ["minListLength", "maxListLength"] {
        for lexical in ["1_0", "١", "\u{00a0}1\u{00a0}", "+", ""] {
            let shapes = shapes(&format!(
                "<urn:S> sh:targetNode <urn:s>; sh:{name} \"{lexical}\"^^xsd:integer."
            ));
            assert!(
                CompiledShapes::compile(shapes, false).is_err(),
                "{name}: {lexical:?}"
            );
        }
        for lexical in [
            "+0001",
            "-0",
            " 1 ",
            "340282366920938463463374607431768211456",
        ] {
            let shapes = shapes(&format!(
                "<urn:S> sh:targetNode <urn:s>; sh:{name} \"{lexical}\"^^xsd:integer."
            ));
            CompiledShapes::compile(shapes, false).unwrap();
        }
    }
}
