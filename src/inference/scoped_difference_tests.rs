//! Sparse common-predecessor subtraction reads A while suppression checks A+B.
use std::collections::HashSet;

use oxigraph::model::*;
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;

use super::super::{Graph, Pending};
use super::ScopedGraph;

fn iri(value: &str) -> NamedNode {
    NamedNode::new(format!("urn:difference:{value}")).unwrap()
}

#[test]
fn sparse_common_predecessors_keep_terminal_terms_and_scoped_visibility() {
    let predicate = NamedNode::new("http://www.w3.org/2002/07/owl#sameAs").unwrap();
    for source in [
        GraphName::DefaultGraph,
        iri("source").into(),
        BlankNode::new("source").unwrap().into(),
    ] {
        for terminal in [
            NamedOrBlankNode::from(iri("terminal")),
            BlankNode::new("terminal").unwrap().into(),
        ] {
            let store = Store::new().unwrap();
            let mut transaction = store.start_transaction().unwrap();
            let destination: GraphName = iri("destination").into();
            let input = [
                (iri("x"), Term::from(iri("b"))),
                (iri("x"), Term::from(terminal.clone())),
                (iri("x"), Term::from(Literal::new_simple_literal("literal"))),
                (iri("b"), Term::from(iri("d"))),
            ];
            for (subject, object) in input {
                transaction.insert_generalized(&GeneralizedQuad::new(
                    subject,
                    predicate.clone(),
                    object,
                    source.clone(),
                ));
            }
            for (subject, object) in [
                (iri("x"), Term::from(iri("z"))),
                (iri("d"), Term::from(iri("z"))),
                (iri("b"), Term::from(terminal.clone())),
            ] {
                transaction.insert_generalized(&GeneralizedQuad::new(
                    subject,
                    predicate.clone(),
                    object,
                    destination.clone(),
                ));
            }
            let view = ScopedGraph {
                transaction: &mut transaction,
                source: source.clone(),
                destination,
            };
            let mut expected = HashSet::new();
            for (head, body) in [
                ("?y owl:sameAs ?x", "?x owl:sameAs ?y"),
                ("?x owl:sameAs ?z", "?x owl:sameAs ?y . ?y owl:sameAs ?z"),
                ("?y ?p ?o", "?x owl:sameAs ?y . ?x ?p ?o"),
                ("?s ?p ?y", "?x owl:sameAs ?y . ?s ?p ?x"),
            ] {
                let query = SparqlEvaluator::new().parse_query(&format!(
                    "PREFIX owl:<http://www.w3.org/2002/07/owl#> CONSTRUCT {{ {head} }} WHERE {{ {body} }}"
                )).unwrap();
                let QueryResults::Graph(triples) = view.query(query).unwrap() else {
                    panic!("expected graph")
                };
                for triple in triples {
                    let triple = triple.unwrap();
                    let quad = GeneralizedQuad::new(
                        triple.subject,
                        triple.predicate,
                        triple.object,
                        GraphName::DefaultGraph,
                    );
                    if !view.contains((&quad).into()).unwrap() {
                        expected.insert(quad);
                    }
                }
            }
            let mut actual = Pending::default();
            super::super::same_as::apply(&view, &mut actual).unwrap();
            let generalized = super::super::same_as_reference_tests::one_round(&view).unwrap();
            assert!(
                expected.is_subset(&generalized),
                "strict public CONSTRUCT control"
            );
            assert_eq!(
                (&actual).into_iter().cloned().collect::<HashSet<_>>(),
                generalized
            );
            assert!(actual.contains(&GeneralizedQuad::new(
                terminal.clone(),
                predicate.clone(),
                Literal::new_simple_literal("literal"),
                GraphName::DefaultGraph
            )));
            assert!(!actual.contains(&GeneralizedQuad::new(
                iri("b"),
                predicate.clone(),
                iri("z"),
                GraphName::DefaultGraph
            )));
        }
    }
}
