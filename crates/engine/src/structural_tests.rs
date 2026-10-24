//! The shared model is the actual input to lowering, with exact engine terms.

use std::collections::HashMap;

use crate::model::{Component, Severity, Target};
use oxigraph::io::RdfFormat;
use oxigraph::model::*;

use crate::functions::Functions;
use crate::graph::{SH, XSD, iri};
use crate::structural::{ShapeDeclaration, read_shape};
use crate::{CompiledShapes, GraphRead, Options, parse_graph};

fn graph(body: &str) -> Dataset {
    parse_graph(
        &format!(
            r#"@prefix sh: <http://www.w3.org/ns/shacl#>.
        @prefix ex: <urn:example:>.
        @prefix xsd: <http://www.w3.org/2001/XMLSchema#>.
        {body}"#
        ),
        RdfFormat::Turtle,
        None,
    )
    .unwrap()
}

fn shape_graph(body: &str) -> crate::IndexedShapesSource {
    crate::IndexedShapesSource::parse_graph(
        &format!(
            r#"@prefix sh: <http://www.w3.org/ns/shacl#>.
        @prefix ex: <urn:example:>.
        @prefix xsd: <http://www.w3.org/2001/XMLSchema#>.
        {body}"#
        ),
        RdfFormat::Turtle,
        None,
    )
    .unwrap()
}

#[test]
fn shared_node_shape_keeps_blank_refs_literal_targets_and_exact_messages() {
    let source = shape_graph(
        r#"_:shape a sh:NodeShape;
        sh:targetNode "plain", "plain"^^xsd:string;
        sh:node _:nested; sh:property _:property;
        sh:severity ex:Blocking; sh:message "Verplicht"@nl;
        sh:order "+0009007199254740993.000000000000000001"^^xsd:decimal.
        _:property sh:path ex:p."#,
    );
    let node: Term = BlankNode::new("shape").unwrap().into();
    let shape = read_shape(
        &source,
        &node,
        &HashMap::new(),
        false,
        &Functions::default(),
        &[],
        Default::default(),
    )
    .unwrap();
    let ShapeDeclaration::Node(shape) = shape else {
        panic!("node shape expected")
    };
    assert_eq!(shape.id, node);
    assert_eq!(
        shape.property_shapes,
        [BlankNode::new("property").unwrap().into()]
    );
    assert!(
        matches!(&shape.components[0], Component::Node(Term::BlankNode(n)) if n.as_str() == "nested")
    );
    assert_eq!(shape.targets.len(), 2);
    assert!(
        shape
            .targets
            .contains(&Target::Node(Literal::new_simple_literal("plain").into()))
    );
    assert!(shape.targets.contains(&Target::Node(
        Literal::new_typed_literal("plain", iri(XSD, "string")).into()
    )));
    assert_eq!(
        shape.messages,
        [Literal::new_language_tagged_literal("Verplicht", "nl")
            .unwrap()
            .into()]
    );
    assert!(
        matches!(shape.severity, Severity::Custom(ref iri) if iri.as_str() == "urn:example:Blocking")
    );
    assert_eq!(
        shape.order,
        Some(
            Literal::new_typed_literal(
                "+0009007199254740993.000000000000000001",
                iri(XSD, "decimal")
            )
            .into()
        )
    );
}

#[test]
fn shared_property_shape_keeps_source_path_and_unbounded_counts() {
    let source = shape_graph(
        r#"_:property sh:path [sh:inversePath ex:p]; sh:targetNode ex:focus;
        sh:minCount "18446744073709551616"^^xsd:integer;
        sh:qualifiedValueShape _:nested; sh:qualifiedMaxCount "-1"^^xsd:integer;
        sh:name "Naam"@nl, "Name"@en; sh:description "Omschrijving"@nl."#,
    );
    let node: Term = BlankNode::new("property").unwrap().into();
    let path_node = crate::graph::one(&source, &node, "path").unwrap().unwrap();
    let paths = HashMap::from([(
        path_node.clone(),
        crate::path::Path::read(&source, &path_node).unwrap(),
    )]);
    let shape = read_shape(
        &source,
        &node,
        &paths,
        false,
        &Functions::default(),
        &[],
        Default::default(),
    )
    .unwrap();
    let ShapeDeclaration::Property(shape) = shape else {
        panic!("property shape expected")
    };
    assert_eq!(shape.path_node, Some(path_node));
    assert_eq!(shape.names.len(), 2);
    assert_eq!(shape.descriptions.len(), 1);
    assert_eq!(
        shape.targets,
        [Target::Node(iri("urn:example:", "focus").into())]
    );
    assert!(
        matches!(&shape.components[0], Component::MinCount(Term::Literal(l)) if l.value() == "18446744073709551616")
    );
    assert!(
        matches!(&shape.components[1], Component::QualifiedValueShape { shape: Term::BlankNode(n), max_count: Some(Term::Literal(l)), .. } if n.as_str() == "nested" && l.value() == "-1")
    );
}

#[test]
fn lowering_uses_the_supplied_structural_constraint_instead_of_rereading_rdf() {
    let source = shape_graph("ex:S sh:targetNode ex:s; sh:hasValue ex:original.");
    let node: Term = iri("urn:example:", "S").into();
    let indices = HashMap::from([(node.clone(), 0)]);
    let constraint = Component::HasValue(iri("urn:example:", "replacement").into());
    let compiled = crate::constraints::lower_one(
        &source,
        &node,
        false,
        &indices,
        constraint,
        Default::default(),
        None,
    )
    .unwrap()
    .unwrap();
    assert!(
        matches!(&compiled, crate::constraints::Constraint::HasValue(value) if value == &Term::from(iri("urn:example:", "replacement")))
    );
}

#[test]
fn compiler_preserves_original_shapes_graph_and_shared_program_reuse() {
    let source = shape_graph(
        r#"_:shape a sh:NodeShape; sh:targetNode ex:s;
        sh:property _:property; sh:message "Parent"@en.
        _:property sh:path ex:p; sh:uniqueLang "1"^^xsd:boolean;
        sh:message "Verplicht"@nl; sh:severity ex:Blocking."#,
    );
    let program =
        CompiledShapes::compile_with_boolean_policy(source.clone(), false, false).unwrap();
    let original: Vec<_> = source
        .matching_quads(None, None, None, None)
        .collect::<crate::Result<_>>()
        .unwrap();
    let retained: Vec<_> = program
        .graph
        .matching_quads(None, None, None, None)
        .collect::<crate::Result<_>>()
        .unwrap();
    assert_eq!(retained, original);
    for _ in 0..2 {
        let mut data = graph("ex:s ex:p 'one'@en, 'two'@en.");
        assert!(
            program
                .validate(&mut data, Options::default())
                .unwrap()
                .conforms
        );
    }
    let program = CompiledShapes::compile(source, false).unwrap();
    let mut data = graph("ex:s ex:p 'one'@en, 'two'@en.");
    let report = program.validate(&mut data, Options::default()).unwrap();
    assert!(!report.conforms);
    assert!(
        report
            .graph
            .iter()
            .any(|q| q.predicate == iri(SH, "resultSeverity").as_ref()
                && q.object == iri("urn:example:", "Blocking").as_ref().into())
    );
}

#[test]
fn sparql_lowering_uses_model_query_prefixes_and_messages() {
    use crate::model::{SparqlConstraint, SparqlQuery};
    let source = shape_graph(
        r#"ex:C sh:select "INVALID SOURCE QUERY"; sh:message "wrong";
        sh:prefixes ex:Wrong.
        ex:Wrong sh:declare [sh:prefix "ex"; sh:namespace "urn:wrong:"^^xsd:anyURI].
        ex:Prefixes sh:declare [sh:prefix "ex"; sh:namespace "urn:example:"^^xsd:anyURI]."#,
    );
    let query = SparqlQuery::new(
        Literal::new_simple_literal("SELECT $this WHERE { FILTER($this = ex:s) }").into(),
    )
    .with_prefixes(iri("urn:example:", "Prefixes").into());
    let mut declaration = SparqlConstraint::new(iri("urn:example:", "C").into(), query);
    let message: Term = Literal::new_language_tagged_literal("Goed", "nl")
        .unwrap()
        .into();
    declaration.messages.push(message.clone());
    let compiled = crate::sparql::lower(&source, declaration, None, &[]).unwrap();
    assert_eq!(compiled.messages, [message]);
    let rows = compiled
        .evaluate(
            &Dataset::new(),
            &source,
            &iri("urn:example:", "S").into(),
            &iri("urn:example:", "s").into(),
            &[],
            None,
            &Functions::default(),
        )
        .unwrap();
    assert_eq!(rows.len(), 1);
    // The pinned pySHACL renders SPARQL messages as plain string literals.
    assert_eq!(
        rows[0].messages,
        [Literal::new_simple_literal("Goed").into()]
    );
}

#[test]
fn custom_component_bindings_keep_distinct_terms_through_model_lowering() {
    let source = shape_graph(
        r#"ex:Component a sh:ConstraintComponent;
        sh:parameter [sh:path <urn:parameter/expected>];
        sh:propertyValidator [sh:select "SELECT $this ?value WHERE { BIND($expected AS ?value) }"].
        ex:P sh:path ex:p; <urn:parameter/expected> "same", "same"^^xsd:string."#,
    );
    let components = crate::components::Component::discover(&source).unwrap();
    let shape: Term = iri("urn:example:", "P").into();
    let application = components[0]
        .read_application(&source, &shape)
        .unwrap()
        .unwrap();
    assert_eq!(application.parameters[0].values.len(), 2);
    let path = crate::path::Path::Predicate(iri("urn:example:", "p"));
    let compiled = components[0]
        .lower(&source, application, Some(&path), &[])
        .unwrap();
    let mut values = Vec::new();
    for constraint in compiled {
        let crate::constraints::Constraint::Sparql(query) = constraint else {
            panic!("SPARQL validator expected")
        };
        let rows = query
            .evaluate(
                &Dataset::new(),
                &source,
                &shape,
                &iri("urn:example:", "s").into(),
                &[],
                None,
                &Functions::default(),
            )
            .unwrap();
        assert_eq!(rows.len(), 1);
        values.push(rows[0].value.clone().unwrap());
    }
    assert_eq!(values.len(), 2);
    assert!(values.contains(&Literal::new_simple_literal("same").into()));
    assert!(values.contains(&Literal::new_typed_literal("same", iri(XSD, "string")).into()));
}

#[test]
fn shared_rule_keeps_blank_expression_condition_and_decimal_order() {
    use crate::model::{ExpressionKind, RuleKind};
    let source = shape_graph(
        r#"ex:S sh:targetNode ex:s; sh:rule _:rule.
        _:rule a sh:TripleRule; sh:order "+0009007199254740993.000001"^^xsd:decimal;
            sh:condition (_:condition); sh:subject sh:this; sh:predicate ex:derived;
            sh:object _:expression.
        _:expression sh:path ex:p.
        _:condition sh:class ex:Ready."#,
    );
    let path: Term = iri("urn:example:", "p").into();
    let paths = HashMap::from([(
        path.clone(),
        crate::path::Path::read(&source, &path).unwrap(),
    )]);
    let declaration = read_shape(
        &source,
        &iri("urn:example:", "S").into(),
        &paths,
        true,
        &Functions::default(),
        &[],
        Default::default(),
    )
    .unwrap();
    let ShapeDeclaration::Node(shape) = declaration else {
        panic!("node shape expected")
    };
    let rule = &shape.rules[0];
    assert_eq!(
        rule.conditions,
        [BlankNode::new("condition").unwrap().into()]
    );
    assert_eq!(
        rule.order,
        Some(Literal::new_typed_literal("+0009007199254740993.000001", iri(XSD, "decimal")).into())
    );
    let RuleKind::Triple(triple) = &rule.kinds[0] else {
        panic!("triple rule expected")
    };
    assert_eq!(
        triple.object.node,
        BlankNode::new("expression").unwrap().into()
    );
    assert!(
        matches!(&triple.object.kind, ExpressionKind::Path { path_node: Some(n), .. } if n == &path)
    );
    let program = CompiledShapes::compile(source, true).unwrap();
    let mut data = graph("ex:s a ex:Ready; ex:p 'kept'.");
    assert!(
        program
            .validate(&mut data, Options::default())
            .unwrap()
            .conforms
    );
    assert!(data.contains(&Quad::new(
        iri("urn:example:", "s"),
        iri("urn:example:", "derived"),
        Literal::new_simple_literal("kept"),
        GraphName::DefaultGraph
    )));
}

#[test]
fn implicit_class_target_keeps_named_and_blank_shape_identity() {
    for class in ["ex:Class", "_:class"] {
        let source = shape_graph(&format!(
            "{class} a sh:NodeShape, <http://www.w3.org/2000/01/rdf-schema#Class>; sh:class ex:Missing."
        ));
        let program = CompiledShapes::compile(source, false).unwrap();
        let mut data = graph(&format!("ex:s a {class}."));
        let report = program.validate(&mut data, Options::default()).unwrap();
        assert!(!report.conforms);
        let focuses = crate::graph::objects(
            &report.graph,
            &report
                .graph
                .quads_for_pattern(None, Some(iri(SH, "focusNode").as_ref()), None, None)
                .next()
                .unwrap()
                .subject
                .into_owned()
                .into(),
            &iri(SH, "focusNode"),
        )
        .unwrap();
        assert_eq!(focuses, [iri("urn:example:", "s").into()]);
    }
}

#[test]
fn unused_validator_queries_are_resolved_only_when_their_role_is_active() {
    for shape in [
        "ex:S sh:targetNode ex:s.",
        "ex:S sh:targetNode ex:s; sh:path ex:p; <urn:parameter/expected> true.",
    ] {
        let source = shape_graph(&format!(
            r#"ex:Component a sh:ConstraintComponent;
            sh:parameter [sh:path <urn:parameter/expected>];
            sh:validator [sh:ask "INVALID UNUSED ASK"];
            sh:nodeValidator [sh:select "INVALID UNUSED NODE SELECT"];
            sh:propertyValidator [sh:select "SELECT $this WHERE {{ FILTER(false) }}"].
            {shape}"#
        ));
        let program = CompiledShapes::compile(source, false).unwrap();
        assert!(
            program
                .validate(&mut Dataset::new(), Options::default())
                .unwrap()
                .conforms
        );
    }
    let source = shape_graph(
        r#"ex:Component a sh:ConstraintComponent;
        sh:parameter [sh:path <urn:parameter/expected>];
        sh:nodeValidator [sh:select "INVALID ACTIVE QUERY"].
        ex:S sh:targetNode ex:s; <urn:parameter/expected> true."#,
    );
    assert!(
        matches!(CompiledShapes::compile(source, false), Err(crate::Error::Invalid(message)) if message.contains("SPARQL parse error"))
    );
}

#[test]
fn public_single_line_is_a_selected_draft_extension_for_node_and_property_shapes() {
    for advanced in [false, true] {
        for property in [false, true] {
            for parameter in ["true", "false", "\"not boolean\"", "1"] {
                let declaration = if property {
                    format!(
                        "ex:S sh:targetNode ex:s; sh:property [sh:path ex:p;sh:singleLine {parameter}]."
                    )
                } else {
                    format!(
                        "ex:S sh:targetNode \"\"\"first\nsecond\"\"\";sh:singleLine {parameter}."
                    )
                };
                let compiled = CompiledShapes::compile(shape_graph(&declaration), advanced);
                if !["true", "false"].contains(&parameter) {
                    assert!(compiled.is_err());
                    continue;
                }
                let program = compiled.unwrap();
                let mut data = graph("ex:s ex:p \"\"\"first\nsecond\"\"\".");
                assert_eq!(
                    program
                        .validate(&mut data, Options::default())
                        .unwrap()
                        .conforms,
                    parameter == "false",
                    "advanced={advanced},property={property},parameter={parameter}"
                );
            }
        }
    }
}
