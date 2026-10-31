//! Focus discovery must not render a wide identifier for every sort comparison.
use oxigraph::io::RdfFormat;
use oxigraph::model::*;
use oxigraph::store::Store;
use pyshacl_turbo_engine::{CompiledShapes, IndexedShapesSource};

#[test]
#[ignore = "isolated requested-allocation budget"]
fn focus_identifier_ordering_has_linear_payload_allocation() {
    const NODES: usize = 256;
    const WIDTH: usize = 2048;
    let store = Store::new().unwrap();
    let class = NamedNode::new_unchecked("urn:Class");
    let predicate = NamedNode::new_unchecked("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");
    let payload = "x".repeat(WIDTH);
    let mut expected = Vec::new();
    let mut writer = store.start_transaction().unwrap();
    for index in 0..NODES {
        let subject = NamedNode::new_unchecked(format!("urn:focus:{index}:{payload}"));
        expected.push(Term::from(subject.clone()));
        writer.insert(&Quad::new(
            subject,
            predicate.clone(),
            class.clone(),
            GraphName::DefaultGraph,
        ));
    }
    writer.commit().unwrap();
    let source = IndexedShapesSource::parse_graph(
        "@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:Shape> a sh:NodeShape; sh:targetClass <urn:Class>.",
        RdfFormat::Turtle,
        None,
    )
    .unwrap();
    let program = CompiledShapes::compile(source, false).unwrap();
    let reader = store.read_snapshot();
    let (actual, allocated) = super::current_thread::measure_allocated(|| {
        program
            .shape_focus_nodes("<urn:Shape>", &reader, false)
            .unwrap()
    });
    // Preserve the old formatted-term ordering, independently of the implementation.
    expected.sort_by_key(ToString::to_string);
    assert_eq!(actual, expected);
    let budget = 4 * NODES * WIDTH;
    println!("FOCUS_ORDER_ALLOCATED {allocated} BUDGET {budget}");
    assert!(
        allocated < budget,
        "Focus discovery repeatedly rendered identifier payloads: {allocated} >= {budget}"
    );
}

#[test]
#[ignore = "isolated requested-allocation budget"]
fn targeting_index_shape_order_has_linear_payload_allocation() {
    const SHAPES: usize = 256;
    const WIDTH: usize = 2048;
    let payload = "x".repeat(WIDTH);
    let mut document = String::from("@prefix sh:<http://www.w3.org/ns/shacl#>.\n");
    let mut expected = Vec::new();
    for index in 0..SHAPES {
        let shape = NamedNode::new_unchecked(format!("urn:shape:{index}:{payload}"));
        document.push_str(&format!(
            "{shape} a sh:NodeShape; sh:targetNode <urn:focus>.\n"
        ));
        expected.push(Term::from(shape));
    }
    let source = IndexedShapesSource::parse_graph(&document, RdfFormat::Turtle, None).unwrap();
    let program = CompiledShapes::compile(source, false).unwrap();
    let store = Store::new().unwrap();
    let reader = store.read_snapshot();
    let skip = std::collections::HashSet::new();
    let (actual, allocated) = super::current_thread::measure_allocated(|| {
        program.targeting_index(&reader, &skip).unwrap()
    });
    expected.sort_by_key(ToString::to_string);
    assert_eq!(actual.len(), 1);
    assert_eq!(
        actual[&Term::from(NamedNode::new_unchecked("urn:focus"))],
        expected
    );
    let budget = 4 * SHAPES * WIDTH;
    println!("TARGET_INDEX_ORDER_ALLOCATED {allocated} BUDGET {budget}");
    assert!(
        allocated < budget,
        "Targeting index repeatedly rendered shape identifiers: {allocated} >= {budget}"
    );
}
