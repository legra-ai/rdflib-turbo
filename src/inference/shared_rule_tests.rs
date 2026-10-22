use datalog_structural_model::{
    Atom, ConstantSymbol, Literal, PredicateSymbol, Term, VariableSymbol,
};
use oxigraph::model::{Dataset, GraphName, NamedNode, Quad};
use oxigraph::sparql::QueryResults;
use rdf_reasoning::RuleId;

#[test]
fn shared_domain_rule_executes_with_exact_backend_terms() {
    let subject = NamedNode::new("urn:shared:subject").unwrap();
    let property = NamedNode::new("urn:shared:property").unwrap();
    let class = NamedNode::new("urn:shared:Class").unwrap();
    let mut dataset = Dataset::new();
    dataset.insert(&Quad::new(
        property.clone(),
        NamedNode::new("http://www.w3.org/2000/01/rdf-schema#domain").unwrap(),
        class.clone(),
        GraphName::DefaultGraph,
    ));
    dataset.insert(&Quad::new(
        subject.clone(),
        property,
        oxigraph::model::Literal::new_simple_literal("exact"),
        GraphName::DefaultGraph,
    ));
    let query = super::shared::CompiledRule::try_new(&RuleId::Rdfs2.definition()).unwrap();
    let QueryResults::Graph(mut triples) = query
        .into_query()
        .on_queryable_dataset(&dataset)
        .execute()
        .unwrap()
    else {
        panic!("expected a graph result")
    };
    let triple = triples.next().unwrap().unwrap();
    assert_eq!(triple.subject.to_string(), subject.to_string());
    assert_eq!(triple.object.to_string(), class.to_string());
    assert!(triples.next().is_none());
}

#[test]
fn shared_adapter_rejects_unimplemented_semantics_before_execution() {
    let mut rule = RuleId::Rdfs2.definition();
    rule.body[0] = Literal::Negative(rule.body[0].atom().unwrap().clone());
    assert!(super::shared::CompiledRule::try_new(&rule).is_err());
    rule = RuleId::Rdfs2.definition();
    rule.head.predicate = PredicateSymbol::new("urn:unsupported:relation");
    assert!(super::shared::CompiledRule::try_new(&rule).is_err());
    rule = RuleId::Rdfs2.definition();
    rule.head.terms[0] = Term::Variable(VariableSymbol::new("s } WHERE { ?x ?y ?z"));
    assert!(super::shared::CompiledRule::try_new(&rule).is_err());
    rule = RuleId::Rdfs2.definition();
    rule.head.terms[0] = Term::TripleTerm(Box::new(Atom {
        predicate: PredicateSymbol::new("urn:p"),
        terms: vec![],
    }));
    assert!(super::shared::CompiledRule::try_new(&rule).is_err());
}

#[test]
fn shared_adapter_validates_terms_even_in_safe_rules() {
    let mut rule = RuleId::Rdfs2.definition();
    rule.head.terms.pop();
    assert!(super::shared::CompiledRule::try_new(&rule).is_err());

    rule = RuleId::Rdfs2.definition();
    let injected = Term::Variable(VariableSymbol::new("s } WHERE { ?x ?y ?z"));
    rule.head.terms[0] = injected.clone();
    let Literal::Positive(atom) = &mut rule.body[1] else {
        panic!("catalogue positive atom")
    };
    atom.terms[0] = injected;
    assert!(rule.is_safe());
    assert!(super::shared::CompiledRule::try_new(&rule).is_err());

    rule = RuleId::Rdfs2.definition();
    rule.head.terms[1] = Term::Constant(ConstantSymbol::new("not an IRI"));
    assert!(super::shared::CompiledRule::try_new(&rule).is_err());

    rule = RuleId::Rdfs2.definition();
    rule.head.terms[0] = Term::Skolem {
        name: ConstantSymbol::new("urn:skolem"),
        args: vec![],
    };
    assert!(super::shared::CompiledRule::try_new(&rule).is_err());
}
