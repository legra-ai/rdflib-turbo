//! Exact synthetic original OWL7.6.2 goldens over the existing generalized Store.
use std::collections::HashSet;

use oxigraph::model::*;
use oxigraph::store::Store;
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    terms: Vec<TermRecord>,
    cases: Vec<Case>,
    union_control: UnionControl,
}
#[derive(Deserialize)]
struct TermRecord(String, String, Option<String>, Option<String>);
#[derive(Deserialize)]
struct Case {
    name: String,
    separate: bool,
    source_scope: usize,
    destination_scope: usize,
    input: Vec<[usize; 3]>,
    destination_initial: Vec<[usize; 3]>,
    expected_source: Vec<[usize; 3]>,
    expected_destination: Vec<[usize; 3]>,
}
#[derive(Deserialize)]
struct UnionControl {
    input: Vec<[usize; 4]>,
    expected: Vec<[usize; 4]>,
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/generalized_owl_original_7_6_2.json"
    ))
    .unwrap()
}

impl Fixture {
    fn term(&self, index: usize) -> Term {
        let TermRecord(kind, value, datatype, language) = &self.terms[index];
        match kind.as_str() {
            "iri" => NamedNode::new_unchecked(value.clone()).into(),
            "blank" => BlankNode::new_unchecked(value.clone()).into(),
            "literal" => match (datatype, language) {
                (_, Some(language)) => {
                    Literal::new_language_tagged_literal_unchecked(value.clone(), language.clone())
                        .into()
                }
                (Some(datatype), None) => Literal::new_typed_literal(
                    value.clone(),
                    NamedNode::new_unchecked(datatype.clone()),
                )
                .into(),
                (None, None) => Literal::new_simple_literal(value.clone()).into(),
            },
            _ => panic!("unknown golden term kind"),
        }
    }

    fn graph(&self, index: usize) -> GraphName {
        match self.term(index) {
            Term::NamedNode(node) => node.into(),
            Term::BlankNode(node) => node.into(),
            _ => panic!("golden graph identifier must be named or blank"),
        }
    }

    fn rows(&self, rows: &[[usize; 3]], graph: GraphName) -> HashSet<GeneralizedQuad> {
        // bounded: a fixed synthetic original closure, at most114statements.
        rows.iter()
            .map(|row| {
                GeneralizedQuad::new(
                    self.term(row[0]),
                    self.term(row[1]),
                    self.term(row[2]),
                    graph.clone(),
                )
            })
            .collect()
    }

    fn quads(&self, rows: &[[usize; 4]]) -> HashSet<GeneralizedQuad> {
        // bounded: the fixed duplicate-context original closure control.
        rows.iter()
            .map(|row| {
                GeneralizedQuad::new(
                    self.term(row[0]),
                    self.term(row[1]),
                    self.term(row[2]),
                    self.graph(row[3]),
                )
            })
            .collect()
    }
}

fn run_case(name: &str, separate: bool) {
    let fixture = fixture();
    let case = fixture
        .cases
        .iter()
        .find(|case| case.name == name && case.separate == separate)
        .unwrap();
    let source = fixture.graph(case.source_scope);
    let destination = fixture.graph(case.destination_scope);
    let store = Store::new().unwrap();
    let mut transaction = store.start_transaction().unwrap();
    for quad in fixture
        .rows(&case.input, source.clone())
        .into_iter()
        .chain(fixture.rows(&case.destination_initial, destination.clone()))
    {
        transaction.insert_generalized(&quad);
    }
    transaction.commit().unwrap();
    let mut transaction = store.start_transaction().unwrap();
    let outcome = super::scoped::expand(
        &mut transaction,
        "owlrl",
        Some(source.clone()),
        destination.clone(),
        super::InferenceRequest::default(),
    );
    assert!(
        outcome.is_ok(),
        "original7.6.2 accepts {name}/separate={separate}; native returned {outcome:?}"
    );
    transaction.commit().unwrap();
    let actual_source = store
        .generalized_quads_for_pattern(None, None, None, Some(source.as_ref()))
        .collect::<Result<HashSet<_>, _>>()
        .unwrap();
    let expected_source = fixture.rows(&case.expected_source, source);
    let actual_destination = store
        .generalized_quads_for_pattern(None, None, None, Some(destination.as_ref()))
        .collect::<Result<HashSet<_>, _>>()
        .unwrap();
    let expected_destination = fixture.rows(&case.expected_destination, destination);
    assert!(
        actual_source == expected_source,
        "source {name}/separate={separate}: missing={:?}, extra={:?}",
        expected_source
            .difference(&actual_source)
            .collect::<Vec<_>>(),
        actual_source
            .difference(&expected_source)
            .collect::<Vec<_>>()
    );
    assert!(
        actual_destination == expected_destination,
        "destination {name}/separate={separate}: missing={:?}, extra={:?}",
        expected_destination
            .difference(&actual_destination)
            .collect::<Vec<_>>(),
        actual_destination
            .difference(&expected_destination)
            .collect::<Vec<_>>()
    );
}

macro_rules! parity_case {
    ($test:ident, $case:literal, $separate:literal) => {
        #[test]
        fn $test() {
            run_case($case, $separate);
        }
    };
}
parity_case!(ordinary_same_scope, "ordinary", false);
parity_case!(ordinary_separate_destination, "ordinary", true);
parity_case!(
    literal_object_typing_same_scope,
    "literal_object_typing",
    false
);
parity_case!(
    literal_object_typing_separate_destination,
    "literal_object_typing",
    true
);
parity_case!(literal_subject_same_scope, "literal_subject", false);
parity_case!(
    literal_subject_separate_destination,
    "literal_subject",
    true
);
parity_case!(
    blank_predicate_cleanup_same_scope,
    "blank_predicate_cleanup",
    false
);
parity_case!(
    blank_predicate_cleanup_separate_destination,
    "blank_predicate_cleanup",
    true
);
parity_case!(
    literal_predicate_retained_same_scope,
    "literal_predicate_retained",
    false
);
parity_case!(
    literal_predicate_retained_separate_destination,
    "literal_predicate_retained",
    true
);
parity_case!(
    new_literal_predicate_refused_same_scope,
    "new_literal_predicate_refused",
    false
);
parity_case!(
    new_literal_predicate_refused_separate_destination,
    "new_literal_predicate_refused",
    true
);

#[test]
fn generalized_union_cleanup_preserves_exact_graph_kinds_and_duplicate_contexts() {
    let fixture = fixture();
    let store = Store::new().unwrap();
    let mut transaction = store.start_transaction().unwrap();
    for quad in fixture.quads(&fixture.union_control.input) {
        transaction.insert_generalized(&quad);
    }
    transaction.commit().unwrap();
    let destination = GraphName::from(BlankNode::new_unchecked("urn:scope"));
    let mut transaction = store.start_transaction().unwrap();
    let outcome = super::expand_transaction(
        &mut transaction,
        "owlrl",
        destination,
        super::InferenceRequest::default(),
    );
    assert!(
        outcome.is_ok(),
        "original union accepts generalized triples: {outcome:?}"
    );
    transaction.commit().unwrap();
    let actual = store
        .generalized_quads_for_pattern(None, None, None, None)
        .collect::<Result<HashSet<_>, _>>()
        .unwrap();
    assert_eq!(actual, fixture.quads(&fixture.union_control.expected));
}
