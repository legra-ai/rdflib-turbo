use oxigraph::model::*;
use oxigraph::sparql::{QueryResults, SparqlEvaluator};

use oxrdf::GeneralizedDataset;
use std::cell::Cell;

use super::expand;

#[test]
fn literal_substitution_plan_joins_on_the_value_key() {
    let data = GeneralizedDataset::new();
    let (result, explanation) = super::literal_values::value_query(Default::default())
        .unwrap()
        .on_queryable_dataset(&data)
        .explain();
    drop(result.unwrap());
    let plan = format!("{explanation:?}");
    assert!(
        plan.contains("Join(HashBuildLeftProbeRight, keys = ?key)"),
        "Literal substitution must have a value-keyed join: {plan}"
    );
    assert_eq!(
        plan.matches("Join(").count(),
        1,
        "The original literal is already bound; only replacement lookup needs a join: {plan}"
    );
}

#[test]
fn literal_value_compiler_rejects_changed_bound_operand_roles() {
    let original = rdf_reasoning::LiteralRuleId::RdfsValueSubstitution.definition();
    // bounded: three malformed variations of the fixed published clause.
    for variation in 0..3 {
        let mut rule = original.clone();
        match variation {
            0 => rule.body.swap(0, 1),
            1 => rule.head.terms.swap(0, 1),
            2 => {
                let datalog_structural_model::Literal::Positive(assertion) = &rule.body[0] else {
                    panic!("published assertion")
                };
                rule.head.terms[2] = assertion.terms[2].clone();
            }
            _ => unreachable!(),
        }
        assert!(rule.is_safe());
        assert!(
            super::shared::CompiledRule::with_literal_values(&rule, SparqlEvaluator::new())
                .is_err(),
            "Unexpected substitution role variation {variation} must fail before execution"
        );
    }
}

#[test]
fn bound_join_keys_preserve_unbound_wildcards_only_when_the_filter_allows_them() {
    // bounded: three guards over two constant rows on each side.
    for (guard, expected) in [
        ("BOUND(?key)", 1),
        ("BOUND(?key) || true", 3),
        ("!BOUND(?key)", 2),
    ] {
        let query = format!(
            r#"SELECT ?left ?right ?key WHERE {{
                {{ SELECT ?left ?key WHERE {{
                    VALUES (?left ?x) {{ ("one" 1) ("unbound" UNDEF) }}
                    BIND (?x AS ?key) FILTER ({guard})
                }} }}
                {{ SELECT ?right ?key WHERE {{
                    VALUES (?right ?y) {{ ("first" 1) ("second" 2) }}
                    BIND (?y AS ?key) FILTER (BOUND(?key))
                }} }}
            }}"#
        );
        let data = GeneralizedDataset::new();
        let QueryResults::Solutions(solutions) = SparqlEvaluator::new()
            .parse_query(&query)
            .unwrap()
            .on_queryable_dataset(&data)
            .execute()
            .unwrap()
        else {
            panic!("Expected SELECT solutions")
        };
        assert_eq!(
            solutions
                .inspect(|row| {
                    row.as_ref().unwrap();
                })
                .count(),
            expected,
            "{guard}"
        );
    }
}

#[test]
fn rdfs_substitutes_equal_string_values_across_datatypes_and_languages() {
    let predicate = NamedNode::new("urn:label").unwrap();
    let xsd = "http://www.w3.org/2001/XMLSchema#";
    // bounded: the seven builtin string-valued term forms and two controls.
    let values = [
        Literal::new_simple_literal("same"),
        Literal::new_language_tagged_literal("same", "en").unwrap(),
        Literal::new_language_tagged_literal("same", "fr").unwrap(),
        Literal::new_typed_literal("same", NamedNode::new(format!("{xsd}string")).unwrap()),
        Literal::new_typed_literal("same", NamedNode::new(format!("{xsd}token")).unwrap()),
        Literal::new_typed_literal("same", NamedNode::new(format!("{xsd}anyURI")).unwrap()),
        Literal::new_typed_literal("same", NamedNode::new(format!("{xsd}language")).unwrap()),
        Literal::new_simple_literal("different"),
        Literal::new_typed_literal("same", NamedNode::new("urn:unknown-datatype").unwrap()),
    ];
    let mut data =
        GeneralizedDataset::from_iter(values.iter().enumerate().map(|(index, value)| {
            GeneralizedQuad::new(
                NamedNode::new(format!("urn:s{index}")).unwrap(),
                predicate.clone(),
                value.clone(),
                GraphName::DefaultGraph,
            )
        }));
    expand(&mut data, "rdfs", super::InferenceRequest::default()).unwrap();
    for (subject, _) in values.iter().enumerate() {
        for (replacement, value) in values.iter().enumerate() {
            let expected = subject == replacement || (subject < 7 && replacement < 7);
            let quad = GeneralizedQuad::new(
                NamedNode::new(format!("urn:s{subject}")).unwrap(),
                predicate.clone(),
                value.clone(),
                GraphName::DefaultGraph,
            );
            assert_eq!(data.contains(&quad), expected, "{quad:?}");
        }
    }
}

#[test]
fn rdfs_substitutes_equal_numeric_values_without_conflating_strings_or_rounding() {
    let xsd = "http://www.w3.org/2001/XMLSchema#";
    // bounded: six equal Python numeric values and four distinct controls.
    let values = [
        ("true", "boolean"),
        ("1", "integer"),
        ("01", "nonNegativeInteger"),
        ("1.0", "decimal"),
        ("1.0", "double"),
        ("1.0", "float"),
        ("2", "integer"),
        ("1", "string"),
        ("0.1", "decimal"),
        ("0.1", "double"),
    ]
    .map(|(lexical, datatype)| {
        Literal::new_typed_literal(lexical, NamedNode::new(format!("{xsd}{datatype}")).unwrap())
    });
    let predicate = NamedNode::new("urn:value").unwrap();
    let mut data = GeneralizedDataset::new();
    for (index, literal) in values.iter().enumerate() {
        data.insert(&GeneralizedQuad::new(
            NamedNode::new(format!("urn:s{index}")).unwrap(),
            predicate.clone(),
            literal.clone(),
            GraphName::DefaultGraph,
        ));
    }
    expand(&mut data, "rdfs", Default::default()).unwrap();
    for (subject, _) in values.iter().enumerate() {
        for (object, literal) in values.iter().enumerate() {
            let expected = (subject < 6 && object < 6) || subject == object;
            assert_eq!(
                data.contains(&GeneralizedQuad::new(
                    NamedNode::new(format!("urn:s{subject}")).unwrap(),
                    predicate.clone(),
                    literal.clone(),
                    GraphName::DefaultGraph
                )),
                expected,
                "numeric substitution {subject} -> {object}"
            );
        }
    }
}

#[test]
fn signaling_nan_is_not_compared_with_itself_or_non_numeric_values() {
    let nan = Literal::new_typed_literal(
        "sNaN",
        NamedNode::new("http://www.w3.org/2001/XMLSchema#decimal").unwrap(),
    );
    let mut data = GeneralizedDataset::from_iter([GeneralizedQuad::new(
        NamedNode::new("urn:s").unwrap(),
        NamedNode::new("urn:p").unwrap(),
        nan,
        GraphName::DefaultGraph,
    )]);
    expand(&mut data, "rdfs", Default::default()).unwrap();
    data.insert(&GeneralizedQuad::new(
        NamedNode::new("urn:s").unwrap(),
        NamedNode::new("urn:q").unwrap(),
        Literal::new_simple_literal("one"),
        GraphName::DefaultGraph,
    ));
    expand(&mut data, "rdfs", Default::default()).unwrap();
    data.insert(&GeneralizedQuad::new(
        NamedNode::new("urn:s").unwrap(),
        NamedNode::new("urn:n").unwrap(),
        Literal::from(1),
        GraphName::DefaultGraph,
    ));
    assert!(matches!(
        expand(&mut data, "rdfs", Default::default()),
        Err(crate::Error::Lexical(
            super::LexicalFailure::DecimalInvalidComparison
        ))
    ));
}

#[derive(Default)]
struct HeadFailure {
    scanned_objects: Cell<usize>,
}
impl super::backend::Graph for HeadFailure {
    fn quads<'a>(
        &'a self,
        _: Option<TermRef<'_>>,
        _: Option<TermRef<'_>>,
        _: Option<TermRef<'_>>,
    ) -> impl Iterator<Item = crate::Result<GeneralizedQuad>> + use<'a> {
        std::iter::once_with(|| {
            self.scanned_objects.set(self.scanned_objects.get() + 1);
            Ok(GeneralizedQuad::new(
                NamedNode::new_unchecked("urn:s"),
                NamedNode::new_unchecked("urn:p"),
                Literal::from(1),
                GraphName::DefaultGraph,
            ))
        })
    }
    fn query(&self, _: oxigraph::sparql::PreparedSparqlQuery) -> crate::Result<QueryResults<'_>> {
        panic!("substitution must not return to strict public projection")
    }
    fn inference_heads(
        &self,
        _: oxigraph::sparql::PreparedSparqlQuery,
    ) -> crate::Result<oxigraph::sparql::QueryTripleIter<'_, GeneralizedTriple>> {
        assert_eq!(
            self.scanned_objects.get(),
            1,
            "numeric preflight must complete"
        );
        Err(crate::Error::Runtime(
            "substitution source read failed".into(),
        ))
    }
    fn insert(&mut self, _: GeneralizedQuad) -> crate::Result<()> {
        panic!("failed substitution must not publish")
    }
    fn cleanup_blank_predicates(&mut self) -> crate::Result<()> {
        panic!("substitution must not perform OWL postprocessing")
    }
}

#[test]
fn rdfs_substitution_propagates_inference_head_errors_without_publication() {
    let result =
        super::literal_values::substitute(&mut HeadFailure::default(), &Default::default());
    assert!(
        matches!(result,Err(crate::Error::Runtime(message)) if message=="substitution source read failed")
    );
}
