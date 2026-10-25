//! Catalogue plan counts and immutable-round grouping parity.
use super::shared::CompiledRule;
use datalog_structural_model::{
    BuiltinFilter, ConstantSymbol, Literal, PredicateSymbol, Term, VariableSymbol,
};
use oxigraph::model::*;
use oxigraph::sparql::QueryResults;
use oxrdf::GeneralizedDataset;
use rdf_reasoning::RuleId;

#[path = "shared_group_failure_tests.rs"]
mod failure_tests;
#[path = "shared_group_oracle_tests.rs"]
mod oracle;
#[path = "shared_group_round_tests.rs"]
mod round_tests;

#[test]
fn exact_catalogue_body_grouping_reduces_only_repeated_body_plans() {
    let actual = ["rdfs", "owlrl", "both"].map(|mode| CompiledRule::for_mode(mode).unwrap().len());
    assert_eq!(actual, [11, 43, 49]);
    // The five existing local OWL queries remain distinct and unchanged.
    assert_eq!(
        CompiledRule::for_mode("both").unwrap().len() + super::OWL_RULES.len(),
        54
    );
}

#[test]
fn catalogue_grouping_keeps_first_body_order_and_every_original_head() {
    let rules = [
        RuleId::Rdfs8.definition(),
        RuleId::Rdfs6.definition(),
        RuleId::Rdfs10.definition(),
    ];
    let queries = CompiledRule::try_catalogue(&rules).unwrap();
    assert_eq!(queries.len(), 2);
    let n = |value| NamedNode::new(value).unwrap();
    let data = GeneralizedDataset::from_iter([
        GeneralizedQuad::new(
            n("urn:C"),
            n("http://www.w3.org/1999/02/22-rdf-syntax-ns#type"),
            n("http://www.w3.org/2000/01/rdf-schema#Class"),
            GraphName::DefaultGraph,
        ),
        GeneralizedQuad::new(
            n("urn:p"),
            n("http://www.w3.org/1999/02/22-rdf-syntax-ns#type"),
            n("http://www.w3.org/1999/02/22-rdf-syntax-ns#Property"),
            GraphName::DefaultGraph,
        ),
    ]);
    let subjects = queries
        .into_iter()
        .map(|query| {
            let QueryResults::Graph(triples) = query.on_queryable_dataset(&data).execute().unwrap()
            else {
                panic!("expected CONSTRUCT")
            };
            triples
                .map(|triple| triple.unwrap().subject)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert_eq!(subjects[0], vec![n("urn:C").into(), n("urn:C").into()]);
    assert_eq!(subjects[1], vec![n("urn:p").into()]);
    // Generic single-rule construction still emits only its own head.
    let QueryResults::Graph(triples) = CompiledRule::try_new(&rules[0])
        .unwrap()
        .into_query()
        .on_queryable_dataset(&data)
        .execute()
        .unwrap()
    else {
        panic!("expected CONSTRUCT")
    };
    assert_eq!(triples.count(), 1);
}

#[test]
fn exact_body_grouping_never_reorders_filters_operands_or_variables() {
    let original = RuleId::Rdfs2.definition();
    let mut left = original.clone();
    let mut right = original.clone();
    let variable = |value| Term::Variable(VariableSymbol::new(value));
    let constant = |value| Term::Constant(ConstantSymbol::new(value));
    let first = Literal::Filter(BuiltinFilter::Ne(variable("s"), constant("urn:exclude")));
    let second = Literal::Filter(BuiltinFilter::Ne(variable("o"), constant("urn:other")));
    left.body.extend([first.clone(), second.clone()]);
    right.body.extend([second, first]);
    assert_eq!(
        CompiledRule::try_catalogue(&[left.clone(), right])
            .unwrap()
            .len(),
        2
    );
    let mut reverse = left.clone();
    reverse.body[2] = Literal::Filter(BuiltinFilter::Ne(constant("urn:exclude"), variable("s")));
    assert_eq!(
        CompiledRule::try_catalogue(&[left, reverse]).unwrap().len(),
        2
    );
    let mut renamed = original.clone();
    for term in renamed.head.terms.iter_mut().chain(
        renamed
            .body
            .iter_mut()
            .filter_map(|literal| match literal {
                Literal::Positive(atom) => Some(atom),
                _ => None,
            })
            .flat_map(|atom| atom.terms.iter_mut()),
    ) {
        if *term == variable("s") {
            *term = variable("renamed");
        }
    }
    assert_eq!(
        CompiledRule::try_catalogue(&[original, renamed])
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn every_original_catalogue_head_and_body_is_validated_before_grouping() {
    let valid = RuleId::Rdfs2.definition();
    // Each malformed rule keeps an already-seen body where possible. Grouping
    // must not hide validation by retaining only the first rule in the group.
    let mut cases = Vec::new(); // bounded: six fixed malformed catalogue controls.
    let mut invalid = valid.clone();
    invalid.head.predicate = PredicateSymbol::new("urn:unsupported");
    cases.push(invalid);
    let mut invalid = valid.clone();
    invalid.head.terms.pop();
    cases.push(invalid);
    let mut invalid = valid.clone();
    invalid.head.terms[1] = Term::Constant(ConstantSymbol::new("not an IRI"));
    cases.push(invalid);
    let mut invalid = valid.clone();
    invalid.head.terms[0] = Term::Variable(VariableSymbol::new("unbound"));
    cases.push(invalid);
    let mut invalid = valid.clone();
    invalid.body[0] = Literal::Negative(invalid.body[0].atom().unwrap().clone());
    cases.push(invalid);
    let mut invalid = valid.clone();
    invalid.body.push(Literal::Filter(BuiltinFilter::Ne(
        Term::Variable(VariableSymbol::new("s")),
        Term::Constant(ConstantSymbol::new("bad filter IRI")),
    )));
    cases.push(invalid);
    for invalid in cases {
        assert!(CompiledRule::try_new(&invalid).is_err());
        assert!(CompiledRule::try_catalogue(&[valid.clone(), invalid]).is_err());
    }
}
