//! Explicit inference heads retain full terms while ordinary CONSTRUCT stays RDF.
use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;

use oxrdf::{
    BlankNode, GeneralizedDataset, GeneralizedQuad, GeneralizedTriple, GraphName, Literal,
    NamedNode, Term,
};

use crate::{QueryEvaluator, QueryResults};

fn head(
    subject: impl Into<Term>,
    predicate: impl Into<Term>,
    object: impl Into<Term>,
) -> GeneralizedTriple {
    GeneralizedTriple {
        subject: subject.into(),
        predicate: predicate.into(),
        object: object.into(),
    }
}

#[test]
fn inference_construct_retains_all_roles_without_widening_default_construct() {
    let terms: [Term; 3] = [
        NamedNode::new("urn:term").unwrap().into(),
        BlankNode::new("term").unwrap().into(),
        Literal::new_simple_literal("term").into(),
    ];
    let mut data = GeneralizedDataset::new();
    let mut expected = HashSet::new();
    for subject in &terms {
        for predicate in &terms {
            let object = Literal::new_simple_literal("value");
            data.insert(&GeneralizedQuad::new(
                subject.clone(),
                predicate.clone(),
                object.clone(),
                GraphName::DefaultGraph,
            ));
            expected.insert(head(subject.clone(), predicate.clone(), object));
        }
    }
    let query = spargebra::SparqlParser::new()
        .parse_query("CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o VALUES ?duplicate { 1 1 } }")
        .unwrap();
    let evaluator = QueryEvaluator::new();
    let actual = evaluator
        .prepare(&query)
        .execute_inference_construct(&data)
        .unwrap()
        .map(Result::unwrap)
        .collect::<HashSet<_>>();
    assert_eq!(actual, expected);
    let QueryResults::Graph(strict) = evaluator.prepare(&query).execute(&data).unwrap() else {
        panic!("strict graph");
    };
    let strict = strict
        .map(|q| {
            let q = q.unwrap();
            head(q.subject, q.predicate, q.object)
        })
        .collect::<HashSet<_>>();
    let expected_strict = expected
        .into_iter()
        .filter(|q| !q.subject.is_literal() && q.predicate.is_named_node())
        .collect::<HashSet<_>>();
    assert_eq!(strict, expected_strict);
}

#[test]
fn inference_construct_shares_constant_variable_ids_and_rejects_unbound_heads() {
    let mut data = GeneralizedDataset::new();
    data.insert(&GeneralizedQuad::new(
        NamedNode::new("urn:s").unwrap(),
        NamedNode::new("urn:p").unwrap(),
        Literal::new_simple_literal("exact"),
        GraphName::DefaultGraph,
    ));
    let query = spargebra::SparqlParser::new().parse_query("CONSTRUCT { <urn:s> <urn:p> ?o . ?s ?p ?o . <urn:s> <urn:p> ?unbound } WHERE { ?s ?p ?o VALUES ?duplicate { 1 1 } }").unwrap();
    let evaluator = QueryEvaluator::new();
    let actual = evaluator
        .prepare(&query)
        .execute_inference_construct(&data)
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    assert_eq!(
        actual,
        vec![head(
            NamedNode::new("urn:s").unwrap(),
            NamedNode::new("urn:p").unwrap(),
            Literal::new_simple_literal("exact")
        )]
    );
}

#[test]
fn inference_construct_generated_blank_labels_stay_per_solution_with_literal_predicates() {
    let query = spargebra::SparqlParser::new().parse_query("CONSTRUCT { _:fresh ?p ?o . _:fresh <urn:q> ?o } WHERE { VALUES (?p ?o) { (\"predicate\" \"value\") (\"predicate\" \"value\") } }").unwrap();
    let evaluator = QueryEvaluator::new();
    let data = GeneralizedDataset::new();
    let actual = evaluator
        .prepare(&query)
        .execute_inference_construct(&data)
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    assert_eq!(actual.len(), 4);
    let subjects = actual
        .iter()
        .map(|q| q.subject.clone())
        .collect::<HashSet<_>>();
    assert_eq!(subjects.len(), 2);
    for subject in subjects {
        assert!(subject.is_blank_node());
        assert_eq!(actual.iter().filter(|q| q.subject == subject).count(), 2);
    }
    assert_eq!(
        actual.iter().filter(|q| q.predicate.is_literal()).count(),
        2
    );
    let QueryResults::Graph(strict) = evaluator.prepare(&query).execute(&data).unwrap() else {
        panic!("strict graph");
    };
    assert_eq!(strict.map(Result::unwrap).count(), 2);
}

#[test]
fn inference_construct_explicitly_refuses_select_ask_and_describe() {
    let evaluator = QueryEvaluator::new();
    let data = GeneralizedDataset::new();
    for source in [
        "SELECT * WHERE { ?s ?p ?o }",
        "ASK { ?s ?p ?o }",
        "DESCRIBE <urn:s>",
    ] {
        let query = spargebra::SparqlParser::new().parse_query(source).unwrap();
        assert!(
            evaluator
                .prepare(&query)
                .execute_inference_construct(&data)
                .is_err()
        );
        assert!(evaluator.prepare(&query).execute(&data).is_ok());
    }
}

#[test]
fn inference_construct_empty_valid_graph_is_not_a_query_form_error() {
    let query = spargebra::SparqlParser::new()
        .parse_query("CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }")
        .unwrap();
    let evaluator = QueryEvaluator::new();
    assert_eq!(
        evaluator
            .prepare(&query)
            .execute_inference_construct(&GeneralizedDataset::new())
            .unwrap()
            .count(),
        0
    );
}

struct BrokenSource {
    externalization: bool,
    calls: Rc<Cell<usize>>,
}
impl<'a> crate::QueryableDataset<'a> for BrokenSource {
    type InternalTerm = Term;
    type Error = std::io::Error;
    fn internal_quads_for_pattern(
        &self,
        _: Option<&Term>,
        _: Option<&Term>,
        _: Option<&Term>,
        _: Option<Option<&Term>>,
    ) -> impl Iterator<Item = Result<crate::InternalQuad<Term>, Self::Error>> + use<'a> {
        let row = || crate::InternalQuad {
            subject: NamedNode::new("urn:s").unwrap().into(),
            predicate: NamedNode::new("urn:p").unwrap().into(),
            object: Literal::new_simple_literal("value").into(),
            graph_name: None,
        };
        std::iter::once(if self.externalization {
            Ok(row())
        } else {
            Err(std::io::Error::other("synthetic head source read failure"))
        })
        .chain(std::iter::once(Ok(row())))
    }
    fn internalize_term(&self, term: Term) -> Result<Term, Self::Error> {
        Ok(term)
    }
    fn externalize_term(&self, _: Term) -> Result<Term, Self::Error> {
        self.calls.set(self.calls.get() + 1);
        Err(std::io::Error::other(
            "synthetic head externalization failure",
        ))
    }
}

#[test]
fn inference_construct_propagates_source_and_externalization_errors() {
    let query = spargebra::SparqlParser::new()
        .parse_query("CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }")
        .unwrap();
    let evaluator = QueryEvaluator::new();
    for externalization in [false, true] {
        let calls = Rc::new(Cell::new(0));
        let mut rows = evaluator
            .prepare(&query)
            .execute_inference_construct(BrokenSource {
                externalization,
                calls: calls.clone(),
            })
            .unwrap();
        let error = rows.next().expect("actual failure row").unwrap_err();
        assert!(
            rows.next().is_none(),
            "no results or callbacks after the first inference failure"
        );
        assert!(
            error.to_string().contains(if externalization {
                "externalization failure"
            } else {
                "source read failure"
            }),
            "{error}"
        );
        assert_eq!(
            calls.get(),
            usize::from(externalization),
            "stop at the first error"
        );
    }
}

#[test]
fn inference_construct_preserves_strict_row_order_constants_and_generated_node_policy() {
    let mut data = GeneralizedDataset::new();
    for value in ["first", "second"] {
        data.insert(&GeneralizedQuad::new(
            NamedNode::new("urn:s").unwrap(),
            NamedNode::new("urn:p").unwrap(),
            Literal::new_simple_literal(value),
            GraphName::DefaultGraph,
        ));
    }
    let query = spargebra::SparqlParser::new()
        .parse_query("CONSTRUCT { ?s ?p ?o . <urn:other> <urn:p> ?o } WHERE { ?s ?p ?o }")
        .unwrap();
    let evaluator = QueryEvaluator::new();
    let QueryResults::Graph(strict) = evaluator.prepare(&query).execute(&data).unwrap() else {
        panic!("strict graph");
    };
    let strict = strict
        .map(|row| {
            let row = row.unwrap();
            head(row.subject, row.predicate, row.object)
        })
        .collect::<Vec<_>>();
    let full = evaluator
        .prepare(&query)
        .execute_inference_construct(&data)
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    assert_eq!(full, strict);
    let query=spargebra::SparqlParser::new().parse_query("CONSTRUCT { <urn:new> <urn:p> \"constant\" . _:fresh <urn:p> ?value } WHERE { VALUES ?value { 1 1 } }").unwrap();
    let rows = evaluator
        .prepare(&query)
        .execute_inference_construct(crate::construct_tests::RejectNewNodes)
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows.iter().filter(|q| q.subject.is_blank_node()).count(), 2);
}

#[cfg(feature = "sparql-12")]
#[test]
fn inference_construct_retains_quoted_roles_and_propagates_nested_template_errors() {
    let evaluator = QueryEvaluator::new();
    let data = GeneralizedDataset::new();
    let query=spargebra::SparqlParser::new().parse_query("CONSTRUCT { ?value <urn:p> ?value } WHERE { VALUES ?value { <<(<urn:a> <urn:q> \"a\"@en--ltr)>> <<(<urn:a> <urn:q> \"a\"@en--rtl)>> } }").unwrap();
    let rows = evaluator
        .prepare(&query)
        .execute_inference_construct(&data)
        .unwrap()
        .map(Result::unwrap)
        .collect::<HashSet<_>>();
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .all(|q| q.subject.is_triple() && q.object.is_triple())
    );
    let QueryResults::Graph(strict) = evaluator.prepare(&query).execute(&data).unwrap() else {
        panic!("strict graph");
    };
    assert_eq!(strict.count(), 0);
    let query = spargebra::SparqlParser::new()
        .parse_query("CONSTRUCT { <urn:s> <urn:p> <<(?s <urn:q> ?o)>> } WHERE { ?s ?p ?o }")
        .unwrap();
    let error = evaluator
        .prepare(&query)
        .execute_inference_construct(BrokenSource {
            externalization: true,
            calls: Rc::new(Cell::new(0)),
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap_err();
    assert!(error.to_string().contains("externalization failure"));
    for source in [
        "CONSTRUCT { <urn:s> <urn:p> <<(?s ?p ?o)>> } WHERE { VALUES (?s ?p ?o) { (\"literal\" <urn:p> \"value\") } }",
        "CONSTRUCT { <urn:s> <urn:p> <<(?s ?p ?o)>> } WHERE { VALUES (?s ?p ?o) { (<urn:s> \"literal\" \"value\") } }",
        "CONSTRUCT { <urn:s> <urn:p> <<(?s ?p ?o)>> } WHERE { VALUES (?s ?o) { (<urn:s> \"value\") } BIND(BNODE() AS ?p) }",
    ] {
        let query = spargebra::SparqlParser::new().parse_query(source).unwrap();
        assert!(
            evaluator
                .prepare(&query)
                .execute_inference_construct(&data)
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .is_err()
        );
        let QueryResults::Graph(strict) = evaluator.prepare(&query).execute(&data).unwrap() else {
            panic!("strict graph");
        };
        assert_eq!(strict.map(Result::unwrap).count(), 0);
    }
}
