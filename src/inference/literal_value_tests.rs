use oxigraph::model::*;
use oxigraph::sparql::{QueryResults, SparqlEvaluator};

use super::expand;

#[test]
fn literal_substitution_plan_joins_on_the_value_key() {
    let data = Dataset::new();
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
        let data = Dataset::new();
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
            solutions.map(|row| row.unwrap()).count(),
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
    let mut data = Dataset::from_iter(values.iter().enumerate().map(|(index, value)| {
        Quad::new(
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
            let quad = Quad::new(
                NamedNode::new(format!("urn:s{subject}")).unwrap(),
                predicate.clone(),
                value.clone(),
                GraphName::DefaultGraph,
            );
            assert_eq!(data.contains(&quad), expected, "{quad}");
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
    let mut data = Dataset::new();
    for (index, literal) in values.iter().enumerate() {
        data.insert(&Quad::new(
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
                data.contains(&Quad::new(
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
