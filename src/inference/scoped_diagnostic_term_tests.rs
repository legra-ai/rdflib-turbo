//! Diagnostic reads preserve generalized terms as the original OWL engine does.
use oxigraph::model::*;
use oxigraph::store::Store;

use super::super::diagnostics::Diagnostics;
use super::ScopedGraph;

fn node(value: &str) -> NamedNode {
    NamedNode::new(value).unwrap()
}
fn run(property: Option<Term>, expected: &str) {
    let rdf_type = node("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");
    let source: GraphName = node("urn:source").into();
    let destination: GraphName = BlankNode::new("source").unwrap().into();
    let focus = Literal::new_simple_literal("focus");
    let mut rows = Vec::new();
    if let Some(property) = property {
        rows.push(GeneralizedQuad::new(
            property.clone(),
            rdf_type,
            node("http://www.w3.org/2002/07/owl#IrreflexiveProperty"),
            source.clone(),
        ));
        rows.push(GeneralizedQuad::new(
            focus.clone(),
            property,
            focus,
            source.clone(),
        ));
    } else {
        rows.push(GeneralizedQuad::new(
            focus,
            rdf_type,
            node("http://www.w3.org/2002/07/owl#Nothing"),
            source.clone(),
        ));
    }
    let store = Store::new().unwrap();
    for row in &rows {
        store.insert_generalized(row).unwrap();
    }
    {
        let mut transaction = store.start_transaction().unwrap();
        let mut graph = ScopedGraph {
            transaction: &mut transaction,
            source,
            destination,
        };
        let mut diagnostics = Diagnostics::new(Vec::new(), 4300);
        diagnostics.round(&graph).unwrap();
        let outcome = diagnostics.finish(&mut graph, true, |_| Ok(false)).unwrap();
        assert_eq!(outcome.error_messages, vec![expected]);
    }
    assert_eq!(store.len().unwrap(), rows.len());
    for row in &rows {
        assert!(store.contains_generalized(row).unwrap());
    }
}
#[test]
fn generalized_literal_subject_nothing_diagnostic() {
    run(None, "focus is defined of type 'Nothing'");
}
#[test]
fn generalized_literal_predicate_irreflexive_diagnostic() {
    run(
        Some(Literal::new_simple_literal("property").into()),
        "Irreflexive property used on focus with property",
    );
}
#[test]
fn generalized_blank_predicate_irreflexive_diagnostic() {
    run(
        Some(BlankNode::new("property").unwrap().into()),
        "Irreflexive property used on focus with property",
    );
}
