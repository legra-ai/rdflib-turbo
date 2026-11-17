use std::collections::HashSet;

use oxigraph::model::{BlankNode, GraphName, Literal, NamedNode, Quad};

use super::*;

#[test]
fn report_terms_keep_exact_literals_and_all_graph_name_kinds() {
    let subject = BlankNode::new("subject").unwrap();
    let predicate = NamedNode::new("urn:p").unwrap();
    let mut data = Dataset::new();
    // bounded: three fixed graph/term forms exercise the transport boundary.
    for (literal, graph) in [
        (Literal::new_simple_literal("word"), GraphName::DefaultGraph),
        (
            Literal::new_typed_literal(
                "word",
                NamedNode::new("http://www.w3.org/2001/XMLSchema#string").unwrap(),
            ),
            NamedNode::new("urn:graph").unwrap().into(),
        ),
        (
            Literal::new_language_tagged_literal("mot", "fr").unwrap(),
            BlankNode::new("graph").unwrap().into(),
        ),
    ] {
        data.insert(&Quad::new(
            subject.clone(),
            predicate.clone(),
            literal,
            graph,
        ));
    }
    let (records, quads) = ReportTerms::encode(&data);
    assert_eq!(records.iter().collect::<HashSet<_>>().len(), records.len());
    assert!(records.contains(&(2, "word".into(), None, None)));
    assert!(records.contains(&(
        2,
        "word".into(),
        Some("http://www.w3.org/2001/XMLSchema#string".into()),
        None,
    )));
    assert!(records.contains(&(2, "mot".into(), None, Some("fr".into()))));
    let graph_records = quads
        .iter()
        .filter_map(|quad| quad.3.map(|index| records[index].clone()))
        .collect::<HashSet<_>>();
    assert_eq!(
        graph_records,
        HashSet::from([
            (0, "urn:graph".into(), None, None),
            (1, "graph".into(), None, None),
        ])
    );
    assert_eq!(quads.iter().filter(|quad| quad.3.is_none()).count(), 1);
}
