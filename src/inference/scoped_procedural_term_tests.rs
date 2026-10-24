//! Original owlrl accepts full-Term list nodes and property/key predicates.
use std::collections::HashSet;

use oxigraph::model::*;
use oxigraph::store::Store;

use super::super::{Pending, traversal};
use super::ScopedGraph;

#[derive(Clone, Copy)]
enum Case {
    LiteralList,
    BlankStep,
    LiteralStep,
    BlankHead,
    BlankKey,
    LiteralKey,
}
fn node(value: &str) -> NamedNode {
    NamedNode::new(format!("urn:synthetic:{value}")).unwrap()
}
fn vocabulary(namespace: &str, value: &str) -> NamedNode {
    NamedNode::new(format!("{namespace}{value}")).unwrap()
}
fn row(s: impl Into<Term>, p: impl Into<Term>, o: impl Into<Term>) -> GeneralizedQuad {
    GeneralizedQuad::new(s, p, o, GraphName::DefaultGraph)
}

fn run(case: Case) {
    let rdf = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
    let owl = "http://www.w3.org/2002/07/owl#";
    let keys = matches!(case, Case::BlankKey | Case::LiteralKey);
    let list: Term = if matches!(case, Case::LiteralList) {
        Literal::new_simple_literal("list-head").into()
    } else {
        node("list").into()
    };
    let predicate: Term = match case {
        Case::BlankStep | Case::BlankKey => BlankNode::new("step").unwrap().into(),
        Case::LiteralStep | Case::LiteralKey => Literal::new_simple_literal("step").into(),
        _ => node("p").into(),
    };
    let head: Term = if matches!(case, Case::BlankHead) {
        BlankNode::new("chain").unwrap().into()
    } else {
        node("chain").into()
    };
    let mut input = vec![
        row(list.clone(), vocabulary(rdf, "first"), predicate.clone()),
        row(
            list.clone(),
            vocabulary(rdf, "rest"),
            vocabulary(rdf, "nil"),
        ),
    ];
    let expected = if keys {
        input.extend([
            row(node("C"), vocabulary(owl, "hasKey"), list),
            row(node("a"), vocabulary(rdf, "type"), node("C")),
            row(node("b"), vocabulary(rdf, "type"), node("C")),
            row(
                node("a"),
                predicate.clone(),
                Literal::new_simple_literal("value"),
            ),
            row(node("b"), predicate, Literal::new_simple_literal("value")),
        ]);
        row(node("a"), vocabulary(owl, "sameAs"), node("b"))
    } else {
        input.extend([
            row(head.clone(), vocabulary(owl, "propertyChainAxiom"), list),
            row(node("s"), predicate, node("o")),
        ]);
        // A blank-predicate consequence remains pending before the separate
        // source-scoped postprocess. It must not be silently narrowed away.
        row(node("s"), head, node("o"))
    };
    let store = Store::new().unwrap();
    let mut transaction = store.start_transaction().unwrap();
    let source: GraphName = node("source").into();
    let destination: GraphName = BlankNode::new("source").unwrap().into();
    for mut q in input {
        q.graph_name = source.clone();
        transaction.insert_generalized(&q);
    }
    let graph = ScopedGraph {
        transaction: &mut transaction,
        source,
        destination,
    };
    let mut pending = Pending::default();
    if keys {
        traversal::keys(
            &graph,
            &mut pending,
            &crate::inference::diagnostics::rdf_list::ListWitnesses::new(4300),
        )
        .unwrap();
    } else {
        traversal::chains(
            &graph,
            &mut pending,
            &crate::inference::diagnostics::rdf_list::ListWitnesses::new(4300),
        )
        .unwrap();
    }
    let actual = pending.0.into_iter().collect::<HashSet<_>>();
    assert!(
        actual.contains(&expected),
        "missing full-Term procedural consequence"
    );
}

#[test]
fn full_term_chain_literal_list_node() {
    run(Case::LiteralList);
}
#[test]
fn full_term_chain_blank_body_predicate() {
    run(Case::BlankStep);
}
#[test]
fn full_term_chain_literal_body_predicate() {
    run(Case::LiteralStep);
}
#[test]
fn full_term_chain_blank_head_before_cleanup() {
    run(Case::BlankHead);
}
#[test]
fn full_term_key_blank_predicate() {
    run(Case::BlankKey);
}
#[test]
fn full_term_key_literal_predicate() {
    run(Case::LiteralKey);
}
