//! Existing scalar report ownership; no source presentation or wire changes.
use oxigraph::io::RdfFormat;
use oxigraph::model::{
    BlankNode, Dataset, GraphName, Literal, NamedNode, NamedOrBlankNode, Quad, Term,
};
use oxrdf::dataset::CanonicalizationAlgorithm;
use pyshacl_turbo_engine::{CompiledShapes, IndexedShapesSource, Options};

const SH: &str = "http://www.w3.org/ns/shacl#";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const REPORT_PEAK_LIMIT: usize = 15 * 1024 * 1024;

fn node(local: &str) -> NamedNode {
    NamedNode::new(format!("urn:report-retirement:{local}")).unwrap()
}

fn add(graph: &mut Dataset, subject: Term, predicate: &str, object: Term) {
    graph.insert(&Quad::new(
        NamedOrBlankNode::try_from(subject).unwrap(),
        NamedNode::new(predicate).unwrap(),
        object,
        GraphName::DefaultGraph,
    ));
}

fn report_arm(nested: bool, query_message: bool, points: usize) {
    // bounded: exactly100 synthetic facts and one fixed-width message.
    const COUNT: usize = 100;
    let message = "m".repeat(points);
    let target = if nested { "Root" } else { "S" };
    let mut shapes = format!(
        "@prefix sh:<{SH}>. <urn:report-retirement:{target}> sh:targetSubjectsOf <urn:report-retirement:p>. "
    );
    if nested {
        shapes.push_str("<urn:report-retirement:Root> sh:node <urn:report-retirement:S>; sh:message \"outer\". ");
    }
    if query_message {
        shapes.push_str(&format!(
            "<urn:report-retirement:S> sh:sparql <urn:report-retirement:Q>. <urn:report-retirement:Q> sh:select \"SELECT $this WHERE {{}}\";sh:message \"{message}\"."
        ));
    } else {
        shapes.push_str(&format!(
            "<urn:report-retirement:S> sh:path <urn:report-retirement:p>;sh:maxCount 0;sh:message \"{message}\"."
        ));
    }
    let source = IndexedShapesSource::parse_graph(&shapes, RdfFormat::Turtle, None).unwrap();
    let program = CompiledShapes::compile(source, true).unwrap();
    let mut data = Dataset::new();
    for index in 0..COUNT {
        data.insert(&Quad::new(
            node(&format!("n{index}")),
            node("p"),
            Literal::from(1),
            GraphName::DefaultGraph,
        ));
    }
    let original = data.clone(); // bounded: this fixed100-fact regression only.
    let (mut report, allocation) =
        super::measure_requested(|| program.validate(&mut data, Options::default()).unwrap());
    let peak = allocation.peak - allocation.live_before;
    assert!(!report.conforms);
    assert_eq!(data, original);
    assert!(report.recursion_warnings.is_empty());

    let mut expected = Dataset::new();
    let root: Term = BlankNode::new("expected-root").unwrap().into();
    add(
        &mut expected,
        root.clone(),
        RDF_TYPE,
        NamedNode::new(format!("{SH}ValidationReport"))
            .unwrap()
            .into(),
    );
    add(
        &mut expected,
        root.clone(),
        &format!("{SH}conforms"),
        Literal::from(false).into(),
    );
    let mut descriptions = Vec::new(); // bounded: one per fixed100 results.
    for index in 0..COUNT {
        let focus: Term = node(&format!("n{index}")).into();
        let leaf: Term = BlankNode::new(format!("leaf{index}")).unwrap().into();
        let parent: Term = if nested {
            BlankNode::new(format!("outer{index}")).unwrap().into()
        } else {
            root.clone()
        };
        add(
            &mut expected,
            parent.clone(),
            &format!("{SH}{}", if nested { "detail" } else { "result" }),
            leaf.clone(),
        );
        let component = if query_message {
            "SPARQLConstraintComponent"
        } else {
            "MaxCountConstraintComponent"
        };
        for (predicate, object) in [
            (
                RDF_TYPE.to_owned(),
                NamedNode::new(format!("{SH}ValidationResult"))
                    .unwrap()
                    .into(),
            ),
            (
                format!("{SH}sourceConstraintComponent"),
                NamedNode::new(format!("{SH}{component}")).unwrap().into(),
            ),
            (format!("{SH}sourceShape"), node("S").into()),
            (
                format!("{SH}resultSeverity"),
                NamedNode::new(format!("{SH}Violation")).unwrap().into(),
            ),
            (format!("{SH}focusNode"), focus.clone()),
            (
                format!("{SH}resultMessage"),
                Literal::new_simple_literal(&message).into(),
            ),
        ] {
            add(&mut expected, leaf.clone(), &predicate, object);
        }
        let location = format!("<urn:report-retirement:n{index}>");
        let mut description = format!(
            "Constraint Violation in {component} ({SH}{component}):\n\tSeverity: sh:Violation\n\tSource Shape: <urn:report-retirement:S>\n\tFocus Node: {location}\n"
        );
        if query_message {
            add(
                &mut expected,
                leaf.clone(),
                &format!("{SH}value"),
                focus.clone(),
            );
            add(
                &mut expected,
                leaf.clone(),
                &format!("{SH}sourceConstraint"),
                node("Q").into(),
            );
            description.push_str(&format!(
                "\tValue Node: {location}\n\tSource Constraint: <urn:report-retirement:Q>\n"
            ));
        } else {
            add(
                &mut expected,
                leaf,
                &format!("{SH}resultPath"),
                node("p").into(),
            );
            description.push_str("\tResult Path: <urn:report-retirement:p>\n");
        }
        description.push_str(&format!("\tMessage: {message}\n"));
        if nested {
            add(
                &mut expected,
                root.clone(),
                &format!("{SH}result"),
                parent.clone(),
            );
            for (predicate, object) in [
                (
                    RDF_TYPE.to_owned(),
                    NamedNode::new(format!("{SH}ValidationResult"))
                        .unwrap()
                        .into(),
                ),
                (
                    format!("{SH}sourceConstraintComponent"),
                    NamedNode::new(format!("{SH}NodeConstraintComponent"))
                        .unwrap()
                        .into(),
                ),
                (format!("{SH}sourceShape"), node("Root").into()),
                (
                    format!("{SH}resultSeverity"),
                    NamedNode::new(format!("{SH}Violation")).unwrap().into(),
                ),
                (format!("{SH}focusNode"), focus.clone()),
                (format!("{SH}value"), focus),
                (
                    format!("{SH}resultMessage"),
                    Literal::new_simple_literal("outer").into(),
                ),
            ] {
                add(&mut expected, parent.clone(), &predicate, object);
            }
            let mut outer = format!(
                "Constraint Violation in NodeConstraintComponent ({SH}NodeConstraintComponent):\n\tSeverity: sh:Violation\n\tSource Shape: <urn:report-retirement:Root>\n\tFocus Node: {location}\n\tValue Node: {location}\n\tMessage: outer\n\tDetails:\n"
            );
            for line in description.lines() {
                outer.push_str(&format!("\t\t{line}\n"));
            }
            description = outer;
        }
        descriptions.push(description);
    }
    descriptions.sort();
    assert_eq!(
        report.text,
        format!(
            "Validation Report\nConforms: False\nResults ({COUNT}):\n{}",
            descriptions.concat()
        )
    );
    expected.canonicalize(CanonicalizationAlgorithm::Unstable);
    report
        .graph
        .canonicalize(CanonicalizationAlgorithm::Unstable);
    assert_eq!(report.graph, expected);
    println!(
        "PUBLIC_REPORT_RETIREMENT nested={nested} query_message={query_message} points={points} count={COUNT} requested={} peak={peak}",
        allocation.allocated
    );
    assert!(
        peak < REPORT_PEAK_LIMIT,
        "Report retained consumed messages: {peak} bytes"
    );
}

#[test]
#[ignore = "global allocation counters require isolated execution"]
fn direct_select_report_retires_owned_messages() {
    report_arm(false, false, 32);
    report_arm(false, false, 64 * 1024);
    report_arm(false, true, 64 * 1024);
}

#[test]
#[ignore = "global allocation counters require isolated execution"]
fn nested_select_report_retires_owned_messages() {
    report_arm(true, true, 64 * 1024);
}
