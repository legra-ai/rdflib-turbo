//! Complete original OWL7.6.2 closures, including every generalized literal row.
use std::collections::HashSet;

use oxigraph::model::*;
use oxigraph::store::Store;
use oxrdf::GeneralizedDataset;
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    terms: Vec<(String, String, Option<String>, Option<String>)>,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    mode: String,
    input: [usize; 3],
    expected: Vec<[usize; 3]>,
}

impl Fixture {
    fn quad(&self, row: &[usize; 3]) -> GeneralizedQuad {
        let term = |index: usize| -> Term {
            let (kind, lexical, datatype, language) = &self.terms[index];
            match kind.as_str() {
                "iri" => NamedNode::new(lexical.clone()).unwrap().into(),
                "blank" => BlankNode::new(lexical.clone()).unwrap().into(),
                "literal" => match (datatype, language) {
                    (_, Some(language)) => {
                        Literal::new_language_tagged_literal(lexical.clone(), language.clone())
                            .unwrap()
                            .into()
                    }
                    (Some(datatype), None) => Literal::new_typed_literal(
                        lexical.clone(),
                        NamedNode::new(datatype.clone()).unwrap(),
                    )
                    .into(),
                    (None, None) => Literal::new_simple_literal(lexical.clone()).into(),
                },
                _ => panic!("unknown literal-resource golden term kind"),
            }
        };
        GeneralizedQuad::new(
            term(row[0]),
            term(row[1]),
            term(row[2]),
            GraphName::DefaultGraph,
        )
    }
}

fn check(name: &str, mode: &str) {
    let fixture: Fixture = serde_json::from_str(include_str!(
        "../../tests/fixtures/literal_resource_original_7_6_2.json"
    ))
    .unwrap();
    let case = fixture
        .cases
        .iter()
        .find(|c| c.name == name && c.mode == mode)
        .unwrap();
    let input = fixture.quad(&case.input);
    let expected: HashSet<_> = case.expected.iter().map(|row| fixture.quad(row)).collect();
    let mut dataset = GeneralizedDataset::from_iter([input.clone()]);
    super::expand(&mut dataset, mode, super::InferenceRequest::default()).unwrap();
    let actual: HashSet<_> = dataset.iter().map(|row| row.into_owned()).collect();
    assert!(
        actual == expected,
        "GeneralizedDataset {name}/{mode}: missing={:?}, extra={:?}",
        expected.difference(&actual).collect::<Vec<_>>(),
        actual.difference(&expected).collect::<Vec<_>>()
    );

    let store = Store::new().unwrap();
    store.insert_generalized(&input).unwrap();
    let mut transaction = store.start_transaction().unwrap();
    super::expand_transaction(
        &mut transaction,
        mode,
        GraphName::DefaultGraph,
        super::InferenceRequest::default(),
    )
    .unwrap();
    transaction.commit().unwrap();
    let actual: HashSet<_> = store
        .generalized_quads_for_pattern(None, None, None, Some(GraphNameRef::DefaultGraph))
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        actual == expected,
        "Store {name}/{mode}: missing={:?}, extra={:?}",
        expected.difference(&actual).collect::<Vec<_>>(),
        actual.difference(&expected).collect::<Vec<_>>()
    );
}

macro_rules! parity_case {
    ($test:ident, $name:literal, $mode:literal) => {
        #[test]
        fn $test() {
            check($name, $mode);
        }
    };
}

parity_case!(plain_rdfs, "plain", "rdfs");
parity_case!(plain_owlrl, "plain", "owlrl");
parity_case!(plain_both, "plain", "both");
parity_case!(language_rdfs, "language", "rdfs");
parity_case!(language_owlrl, "language", "owlrl");
parity_case!(language_both, "language", "both");
parity_case!(explicit_string_rdfs, "explicit_string", "rdfs");
parity_case!(explicit_string_owlrl, "explicit_string", "owlrl");
parity_case!(explicit_string_both, "explicit_string", "both");
parity_case!(integer_rdfs, "integer", "rdfs");
parity_case!(integer_owlrl, "integer", "owlrl");
parity_case!(integer_both, "integer", "both");
