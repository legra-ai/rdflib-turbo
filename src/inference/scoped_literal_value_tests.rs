//! Exact original RDFS one-time graph sets across full-Term roles and scopes.
use super::super::backend::Graph;
use super::{QueryResults, ScopedGraph};
use oxigraph::model::*;
use oxigraph::store::Store;
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Deserialize)]
struct Fixture {
    terms: Vec<(String, String, Option<String>, Option<String>)>,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    subject: usize,
    predicate: usize,
    separate: bool,
    input: Vec<[usize; 3]>,
    destination_initial: Vec<[usize; 3]>,
    expected_source: Vec<[usize; 3]>,
    expected_destination: Vec<[usize; 3]>,
}
impl Fixture {
    fn term(&self, index: usize) -> Term {
        let (kind, value, datatype, language) = &self.terms[index];
        match kind.as_str() {
            "iri" => NamedNode::new_unchecked(value).into(),
            "blank" => BlankNode::new_unchecked(value).into(),
            "literal" => match (datatype, language) {
                (_, Some(language)) => Literal::new_language_tagged_literal(value, language)
                    .unwrap()
                    .into(),
                (Some(datatype), None) => {
                    Literal::new_typed_literal(value, NamedNode::new_unchecked(datatype)).into()
                }
                (None, None) => Literal::new_simple_literal(value).into(),
            },
            _ => panic!("unknown original term kind"),
        }
    }
    fn rows(&self, rows: &[[usize; 3]], graph: &GraphName) -> HashSet<GeneralizedQuad> {
        // bounded: at most five rows in this fixed original one-time fixture.
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
}

fn run_case(subject_kind: &str, predicate_kind: &str, separate: bool) {
    let fixture: Fixture = serde_json::from_str(include_str!(
        "../../tests/fixtures/generalized_rdfs_substitution_original_7_6_2.json"
    ))
    .unwrap();
    let case = fixture
        .cases
        .iter()
        .find(|case| {
            fixture.terms[case.subject].0 == subject_kind
                && fixture.terms[case.predicate].0 == predicate_kind
                && case.separate == separate
        })
        .unwrap();
    let source: GraphName = NamedNode::new_unchecked("urn:scope").into();
    let destination = if separate {
        BlankNode::new_unchecked("urn:scope").into()
    } else {
        source.clone()
    };
    let store = Store::new().unwrap();
    let mut transaction = store.start_transaction().unwrap();
    for quad in fixture
        .rows(&case.input, &source)
        .into_iter()
        .chain(fixture.rows(&case.destination_initial, &destination))
    {
        transaction.insert_generalized(&quad);
    }
    {
        let mut graph = ScopedGraph {
            transaction: &mut transaction,
            source: source.clone(),
            destination: destination.clone(),
        };
        // Ordinary public projection remains strict for this very same body.
        let QueryResults::Graph(strict) = graph
            .query(crate::inference::literal_values::value_query(Default::default()).unwrap())
            .unwrap()
        else {
            panic!("strict CONSTRUCT")
        };
        for triple in strict {
            let triple = triple.unwrap();
            assert!(matches!(
                triple.subject,
                NamedOrBlankNode::NamedNode(_) | NamedOrBlankNode::BlankNode(_)
            ));
        }
        crate::inference::literal_values::substitute(&mut graph, &Default::default()).unwrap();
    }
    let actual_source = transaction
        .generalized_quads_for_pattern(None, None, None, Some(source.as_ref()))
        .collect::<std::result::Result<HashSet<_>, _>>()
        .unwrap();
    let actual_destination = transaction
        .generalized_quads_for_pattern(None, None, None, Some(destination.as_ref()))
        .collect::<std::result::Result<HashSet<_>, _>>()
        .unwrap();
    assert_eq!(
        actual_source,
        fixture.rows(&case.expected_source, &source),
        "source {subject_kind}/{predicate_kind}/separate={separate}"
    );
    assert_eq!(
        actual_destination,
        fixture.rows(&case.expected_destination, &destination),
        "destination {subject_kind}/{predicate_kind}/separate={separate}"
    );
}

macro_rules! cases {
    ($same:ident,$separate:ident,$subject:literal,$predicate:literal) => {
        #[test]
        fn $same() {
            run_case($subject, $predicate, false)
        }
        #[test]
        fn $separate() {
            run_case($subject, $predicate, true)
        }
    };
}
cases!(named_named_same, named_named_separate, "iri", "iri");
cases!(named_blank_same, named_blank_separate, "iri", "blank");
cases!(named_literal_same, named_literal_separate, "iri", "literal");
cases!(blank_named_same, blank_named_separate, "blank", "iri");
cases!(blank_blank_same, blank_blank_separate, "blank", "blank");
cases!(
    blank_literal_same,
    blank_literal_separate,
    "blank",
    "literal"
);
cases!(literal_named_same, literal_named_separate, "literal", "iri");
cases!(
    literal_blank_same,
    literal_blank_separate,
    "literal",
    "blank"
);
cases!(
    literal_literal_same,
    literal_literal_separate,
    "literal",
    "literal"
);
