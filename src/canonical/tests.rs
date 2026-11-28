use std::convert::Infallible;

use oxigraph::model::vocab::xsd;
use oxigraph::model::{BlankNode, GeneralizedQuad, GraphName, Literal, NamedNode, Quad};

use super::{CanonicalForm, CanonicalRequest, CanonicalScope, TermIdentity};
use crate::Error;

fn node(name: &str) -> NamedNode {
    NamedNode::new(format!("http://example.com/{name}")).unwrap()
}

fn quads(quads: Vec<Quad>) -> impl Iterator<Item = Result<GeneralizedQuad, Infallible>> {
    quads
        .into_iter()
        .map(|quad| Ok(GeneralizedQuad::from(quad)))
}

fn render(input: Vec<Quad>, scope: &CanonicalScope, request: CanonicalRequest) -> String {
    CanonicalForm::collect(quads(input), scope, request.identity())
        .unwrap()
        .render(request)
        .unwrap()
}

fn sha256(identity: &str) -> CanonicalRequest {
    CanonicalRequest::try_new("sha256", 1_000_000, identity).unwrap()
}

fn clique(size: usize) -> Vec<Quad> {
    let nodes = (0..size).map(|_| BlankNode::default()).collect::<Vec<_>>();
    let mut result = Vec::new();
    for subject in &nodes {
        for object in &nodes {
            result.push(Quad::new(
                subject.clone(),
                node("p"),
                object.clone(),
                GraphName::DefaultGraph,
            ));
        }
    }
    result
}

#[test]
fn canonical_output_keeps_the_stored_lexical_form() {
    let literal = Literal::new_typed_literal("01", xsd::INTEGER);
    let input = vec![Quad::new(
        BlankNode::default(),
        node("p"),
        literal,
        GraphName::DefaultGraph,
    )];
    assert_eq!(
        render(input, &CanonicalScope::Dataset, sha256("rdf11")),
        "_:c14n0 <http://example.com/p> \"01\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n"
    );
}

#[test]
fn explicit_string_identity_depends_on_the_requested_identity() {
    let typed = Literal::new_typed_literal("a", xsd::STRING);
    let input = || {
        vec![Quad::new(
            node("s"),
            node("p"),
            typed.clone(),
            GraphName::DefaultGraph,
        )]
    };
    let rdf11 = render(input(), &CanonicalScope::Dataset, sha256("rdf11"));
    assert_eq!(
        rdf11,
        "<http://example.com/s> <http://example.com/p> \"a\" .\n"
    );
    let exact = render(input(), &CanonicalScope::Dataset, sha256("exact"));
    assert_eq!(
        exact,
        format!("<http://example.com/s> <http://example.com/p> {typed} .\n")
    );
}

#[test]
fn graph_scope_drops_graph_names_and_dataset_scope_keeps_them() {
    let graph = GraphName::from(node("g"));
    let input = || vec![Quad::new(node("s"), node("p"), node("o"), graph.clone())];
    let line = "<http://example.com/s> <http://example.com/p> <http://example.com/o>";
    assert_eq!(
        render(
            input(),
            &CanonicalScope::Graph(graph.clone()),
            sha256("exact")
        ),
        format!("{line} .\n")
    );
    assert_eq!(
        render(input(), &CanonicalScope::Dataset, sha256("exact")),
        format!("{line} <http://example.com/g> .\n")
    );
    assert_eq!(
        CanonicalScope::Graph(graph).pattern(),
        Some(node("g").as_ref().into())
    );
}

#[test]
fn sha384_changes_only_the_tie_breaking_hashes() {
    let input = clique(3);
    let sha256 = render(input.clone(), &CanonicalScope::Dataset, sha256("exact"));
    let sha384 = CanonicalRequest::try_new("sha384", 1_000_000, "exact").unwrap();
    assert_eq!(
        render(input, &CanonicalScope::Dataset, sha384)
            .lines()
            .count(),
        9
    );
    assert_eq!(sha256.lines().count(), 9);
}

#[test]
fn poison_cliques_exhaust_the_budget_instead_of_hanging() {
    let form = CanonicalForm::collect(
        quads(clique(10)),
        &CanonicalScope::Dataset,
        TermIdentity::Exact,
    )
    .unwrap();
    let request = CanonicalRequest::try_new("sha256", 10_000, "exact").unwrap();
    match form.render(request) {
        Err(Error::CanonicalBudget(exceeded)) => assert_eq!(exceeded.limit().get(), 10_000),
        other => panic!("expected an exhausted budget, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn invalid_requests_are_typed_value_errors() {
    for (hash, budget, identity) in [
        ("md5", 1, "exact"),
        ("sha256", 0, "exact"),
        ("sha256", 1, "loose"),
    ] {
        assert!(matches!(
            CanonicalRequest::try_new(hash, budget, identity),
            Err(Error::Invalid(_))
        ));
    }
}

#[test]
fn generalized_statements_are_rejected() {
    let quad = GeneralizedQuad::new(
        Literal::new_simple_literal("s"),
        node("p"),
        node("o"),
        GraphName::DefaultGraph,
    );
    let input = std::iter::once(Ok::<_, Infallible>(quad));
    assert!(matches!(
        CanonicalForm::collect(input, &CanonicalScope::Dataset, TermIdentity::Exact),
        Err(Error::Invalid(_))
    ));
}
