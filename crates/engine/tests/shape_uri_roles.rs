//! Programmatic named shape roles do not impose the serialized IRI grammar.
use oxigraph::model::*;
use oxigraph::store::Store;
use pyshacl_turbo_engine::{CompiledShapes, IndexedShapesSource, Options};

const SH: &str = "http://www.w3.org/ns/shacl#";
const VALUES: [&str; 5] = [
    "",
    "relative",
    "relative/path",
    "not a standard IRI",
    "urn:absolute",
];

fn source(role: &str, lexical: &str, matching: bool) -> IndexedShapesSource {
    let store = Store::new().unwrap();
    let shape = NamedNode::new("urn:shape").unwrap();
    let value = NamedNode::new_unchecked(lexical);
    let target: Term = if role == "datatype" {
        if matching {
            Literal::new_typed_literal("value", value.clone()).into()
        } else {
            Literal::new_simple_literal("value").into()
        }
    } else {
        NamedNode::new("urn:focus").unwrap().into()
    };
    let mut transaction = store.start_transaction().unwrap();
    let mut insert = |predicate: &str, object: Term| {
        transaction.insert(&Quad::new(
            shape.clone(),
            NamedNode::new(format!("{SH}{predicate}")).unwrap(),
            object,
            GraphName::DefaultGraph,
        ));
    };
    if role == "targetClass" {
        insert(role, value.into());
        insert("class", NamedNode::new("urn:Required").unwrap().into());
    } else {
        insert("targetNode", target);
        insert(role, value.into());
        if role == "path" {
            insert("minCount", Literal::from(1).into());
        }
    }
    transaction.commit().unwrap();
    IndexedShapesSource::new(store.read_snapshot(), Some(GraphName::DefaultGraph))
}

#[test]
fn all_named_shape_role_lexicals_compile_and_evaluate_without_data_decoder_changes() {
    for lexical in VALUES {
        for role in ["path", "class", "targetClass", "datatype"] {
            let program = CompiledShapes::compile(source(role, lexical, false), false).unwrap();
            let report = program
                .validate(&mut Dataset::new(), Options::default())
                .unwrap();
            assert_eq!(
                report.conforms,
                role == "targetClass",
                "{role}: {lexical:?}"
            );
            if role == "path" {
                assert!(
                    report
                        .graph
                        .quads_for_pattern(
                            None,
                            Some(NamedNodeRef::new_unchecked(&format!("{SH}resultPath"))),
                            Some(NamedNodeRef::new_unchecked(lexical).into()),
                            None,
                        )
                        .next()
                        .is_some()
                );
                if lexical.is_empty() {
                    assert!(!report.text.contains("\tResult Path:"));
                    assert!(
                        report
                            .text
                            .contains("\tMessage: Less than 1 values on <urn:focus>\n")
                    );
                }
            }
        }
        let program = CompiledShapes::compile(source("datatype", lexical, true), false).unwrap();
        assert!(
            program
                .validate(&mut Dataset::new(), Options::default())
                .unwrap()
                .conforms
        );
    }
}

#[test]
fn datatype_violation_messages_name_the_original_named_role() {
    for lexical in VALUES {
        let program = CompiledShapes::compile(source("datatype", lexical, false), false).unwrap();
        let report = program
            .validate(&mut Dataset::new(), Options::default())
            .unwrap();
        let displayed = if lexical.contains(' ') {
            lexical.to_owned()
        } else {
            format!("<{lexical}>")
        };
        let expected = format!("Value is not Literal with datatype {displayed}");
        assert!(
            report
                .graph
                .quads_for_pattern(
                    None,
                    Some(NamedNodeRef::new_unchecked(&format!("{SH}resultMessage"))),
                    Some(Literal::new_simple_literal(&expected).as_ref().into()),
                    None,
                )
                .next()
                .is_some(),
            "missing exact datatype diagnostic: {expected}"
        );
        assert!(report.text.contains(&format!("\tMessage: {expected}\n")));
    }
}
