use datalog_structural_model::{
    AggregateBody, Aggregator, Atom, BindExpr, BuiltinFilter, ConstantSymbol, Literal,
    PredicateSymbol, Rule, Term, VariableSymbol,
};
use oxigraph::model::{Dataset, GraphName, NamedNode, NamedOrBlankNode, Quad, Triple};
use oxigraph::sparql::{PreparedSparqlQuery, QueryEvaluationError, QueryResults};
use rdf_reasoning::RuleId;

fn heads(
    query: PreparedSparqlQuery,
    data: &Dataset,
) -> impl Iterator<Item = Result<Triple, QueryEvaluationError>> + '_ {
    let QueryResults::Solutions(rows) = query.on_queryable_dataset(data).execute().unwrap() else {
        panic!("expected shared head bindings")
    };
    rows.map(|row| {
        let row = row?;
        Ok(Triple::new(
            NamedOrBlankNode::try_from(row.get("__head_s").unwrap().clone()).unwrap(),
            NamedNode::try_from(row.get("__head_p").unwrap().clone()).unwrap(),
            row.get("__head_o").unwrap().clone(),
        ))
    })
}

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
    let mut triples = heads(query.into_query(), &dataset);
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

#[test]
fn shared_inequality_compares_exact_term_identity() {
    let variable = |name| Term::Variable(VariableSymbol::new(name));
    let atom = Atom {
        predicate: PredicateSymbol::new(rdf_reasoning::TRIPLE_RELATION),
        terms: vec![variable("s"), variable("p"), variable("o")],
    };
    let rule = datalog_structural_model::Rule::try_new(
        atom.clone(),
        vec![
            Literal::Positive(atom),
            Literal::Filter(BuiltinFilter::Ne(
                variable("o"),
                Term::Constant(ConstantSymbol::new("urn:identity:equal")),
            )),
        ],
    )
    .unwrap();
    let mut dataset = Dataset::new();
    let subject = NamedNode::new("urn:identity:s").unwrap();
    let predicate = NamedNode::new("urn:identity:p").unwrap();
    for object in ["urn:identity:equal", "urn:identity:different"] {
        dataset.insert(&Quad::new(
            subject.clone(),
            predicate.clone(),
            NamedNode::new(object).unwrap(),
            GraphName::DefaultGraph,
        ));
    }
    let query = super::shared::CompiledRule::try_new(&rule).unwrap();
    let mut triples = heads(query.into_query(), &dataset);
    assert_eq!(
        triples.next().unwrap().unwrap().object.to_string(),
        "<urn:identity:different>"
    );
    assert!(triples.next().is_none());
}

#[test]
fn shared_inequality_retains_distinct_numeric_lexical_forms() {
    let variable = |name| Term::Variable(VariableSymbol::new(name));
    let atom = |object| Atom {
        predicate: PredicateSymbol::new(rdf_reasoning::TRIPLE_RELATION),
        terms: vec![variable("s"), variable("p"), variable(object)],
    };
    let rule = datalog_structural_model::Rule::try_new(
        atom("left"),
        vec![
            Literal::Positive(atom("left")),
            Literal::Positive(atom("right")),
            Literal::Filter(BuiltinFilter::Ne(variable("left"), variable("right"))),
        ],
    )
    .unwrap();
    let mut dataset = Dataset::new();
    for lexical in ["1", "01"] {
        dataset.insert(&Quad::new(
            NamedNode::new("urn:numeric:s").unwrap(),
            NamedNode::new("urn:numeric:p").unwrap(),
            oxigraph::model::Literal::new_typed_literal(
                lexical,
                NamedNode::new("http://www.w3.org/2001/XMLSchema#integer").unwrap(),
            ),
            GraphName::DefaultGraph,
        ));
    }
    let query = super::shared::CompiledRule::try_new(&rule).unwrap();
    let triples = heads(query.into_query(), &dataset);
    assert_eq!(triples.count(), 2);
}

#[test]
fn shared_owl_mode_executes_restrictions_and_reference_schema() {
    let n = |name: &str| NamedNode::new(name.to_owned()).unwrap();
    let rdf = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
    let owl = "http://www.w3.org/2002/07/owl#";
    let mut dataset = Dataset::new();
    for (subject, predicate, object) in [
        ("urn:R", format!("{owl}onProperty"), "urn:p".to_owned()),
        ("urn:R", format!("{owl}hasValue"), "urn:v".to_owned()),
        ("urn:a", format!("{rdf}type"), "urn:R".to_owned()),
        ("urn:p", format!("{rdf}type"), format!("{rdf}Property")),
        ("urn:C", format!("{owl}equivalentClass"), "urn:C".to_owned()),
    ] {
        dataset.insert(&Quad::new(
            n(subject),
            n(&predicate),
            n(&object),
            GraphName::DefaultGraph,
        ));
    }
    let mut value = false;
    let mut self_equivalent = false;
    for query in super::shared::CompiledRule::for_mode("owlrl").unwrap() {
        let triples = heads(query, &dataset);
        for triple in triples {
            let triple = triple.unwrap();
            value |= triple.subject == n("urn:a").into()
                && triple.predicate == n("urn:p")
                && triple.object == n("urn:v").into();
            self_equivalent |= triple.subject == n("urn:p").into()
                && triple.predicate == n(&format!("{owl}equivalentProperty"))
                && triple.object == n("urn:p").into();
            assert_ne!(
                triple,
                oxigraph::model::Triple::new(
                    n("urn:C"),
                    n("http://www.w3.org/2000/01/rdf-schema#subClassOf"),
                    n("urn:C")
                )
            );
        }
    }
    assert!(value, "shared hasValue consequence missing");
    assert!(
        self_equivalent,
        "versioned rdf:Property consequence missing"
    );
}

#[test]
fn shared_list_relation_reaches_cycles_and_branches_without_crossing_lists() {
    let variable = |name| Term::Variable(VariableSymbol::new(name));
    let constant = |name| Term::Constant(ConstantSymbol::new(name));
    let rule = datalog_structural_model::Rule::try_new(
        Atom {
            predicate: PredicateSymbol::new(rdf_reasoning::TRIPLE_RELATION),
            terms: vec![
                variable("member"),
                constant("urn:selected"),
                constant("urn:yes"),
            ],
        },
        vec![
            Literal::Positive(Atom {
                predicate: PredicateSymbol::new(rdf_reasoning::TRIPLE_RELATION),
                terms: vec![
                    constant("urn:owner"),
                    constant("urn:list"),
                    variable("list"),
                ],
            }),
            Literal::Positive(Atom {
                predicate: PredicateSymbol::new("urn:rdf-reasoning:list-member"),
                terms: vec![variable("list"), variable("member")],
            }),
        ],
    )
    .unwrap();
    let mut dataset = Dataset::new();
    let first = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
    let rest = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
    for (subject, predicate, object) in [
        ("urn:owner", "urn:list", "urn:l"),
        ("urn:l", first, "urn:A"),
        ("urn:l", rest, "urn:tail"),
        ("urn:l", rest, "urn:branch"),
        ("urn:tail", first, "urn:B"),
        ("urn:tail", rest, "urn:l"),
        ("urn:branch", first, "urn:C"),
        ("urn:other", first, "urn:unrelated"),
    ] {
        dataset.insert(&Quad::new(
            NamedNode::new(subject).unwrap(),
            NamedNode::new(predicate).unwrap(),
            NamedNode::new(object).unwrap(),
            GraphName::DefaultGraph,
        ));
    }
    let query = super::shared::CompiledRule::try_new(&rule).unwrap();
    let triples = heads(query.into_query(), &dataset);
    // bounded: the three reachable members in this eight-triple fixture.
    let subjects = triples
        .map(|triple| triple.unwrap().subject.to_string())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        subjects,
        ["<urn:A>", "<urn:B>", "<urn:C>"].map(str::to_owned).into()
    );
}

#[test]
fn shared_list_consumers_compile_but_auxiliary_heads_and_bad_arity_fail() {
    for id in rdf_reasoning::ListRuleId::ALL {
        let result = super::shared::CompiledRule::try_new(&id.definition());
        assert_eq!(
            result.is_ok(),
            !matches!(
                id,
                rdf_reasoning::ListRuleId::MemberFirst | rdf_reasoning::ListRuleId::MemberRest
            )
        );
    }
    let mut rule = rdf_reasoning::ListRuleId::ClsOo.definition();
    let Literal::Positive(member) = &mut rule.body[1] else {
        panic!("list membership atom")
    };
    member
        .terms
        .push(Term::Constant(ConstantSymbol::new("urn:extra")));
    assert!(rule.is_safe());
    assert!(super::shared::CompiledRule::try_new(&rule).is_err());
}

#[test]
fn shared_aggregate_exports_validate_before_adapter_rejection() {
    let total = VariableSymbol::new("total");
    let copy = VariableSymbol::new("copy");
    let input = VariableSymbol::new("input");
    // bounded: one fixed clause with a single aggregate and subsequent bind.
    let rule = Rule::try_new(
        Atom {
            predicate: PredicateSymbol::new(rdf_reasoning::TRIPLE_RELATION),
            terms: vec![
                Term::Constant(ConstantSymbol::new("urn:shared:subject")),
                Term::Constant(ConstantSymbol::new("urn:shared:count")),
                Term::Variable(copy.clone()),
            ],
        },
        vec![
            Literal::Aggregate(AggregateBody {
                result_var: total.clone(),
                aggregator: Aggregator::Count,
                input_var: input.clone(),
                group_by: vec![],
                inner_body: vec![Literal::Positive(Atom {
                    predicate: PredicateSymbol::new("urn:shared:input-relation"),
                    terms: vec![
                        Term::Variable(VariableSymbol::new("subject")),
                        Term::Constant(ConstantSymbol::new("urn:shared:input")),
                        Term::Variable(input),
                    ],
                })],
            }),
            Literal::Bind {
                var: copy,
                expr: BindExpr::Term(Term::Variable(total)),
            },
        ],
    )
    .expect("shared aggregate exports must bind subsequent expressions");
    assert!(rule.is_safe());
    assert!(matches!(
        super::shared::CompiledRule::try_new(&rule),
        Err(crate::Error::Unsupported(_))
    ));
}
