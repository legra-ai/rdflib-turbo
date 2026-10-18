use oxrdf::{Dataset, GraphName, Literal, NamedNode, Quad, Term, Triple};
use spareval::{QueryEvaluator, QueryResults};
use std::collections::HashSet;

fn results(query: &str, data: &Dataset) -> Vec<Triple> {
    let query = spargebra::SparqlParser::new().parse_query(query).unwrap();
    let evaluator = QueryEvaluator::new();
    let QueryResults::Graph(rows) = evaluator.prepare(&query).execute(data).unwrap() else {
        panic!("expected CONSTRUCT graph")
    };
    rows.map(Result::unwrap).collect()
}

#[test]
fn construct_internal_dedup_keeps_exact_literal_identity_and_mixed_templates() {
    let s = NamedNode::new("urn:s").unwrap();
    let p = NamedNode::new("urn:p").unwrap();
    let integer = NamedNode::new("http://www.w3.org/2001/XMLSchema#integer").unwrap();
    let values = [
        Literal::new_typed_literal("01", integer.clone()),
        Literal::new_typed_literal("1", integer),
        Literal::new_simple_literal("hello"),
        Literal::new_typed_literal(
            "hello",
            NamedNode::new("http://www.w3.org/2001/XMLSchema#string").unwrap(),
        ),
    ];
    let mut data = Dataset::new();
    for value in &values {
        data.insert(
            Quad::new(s.clone(), p.clone(), value.clone(), GraphName::DefaultGraph).as_ref(),
        );
    }
    let rows = results(
        "CONSTRUCT { ?s ?p ?o . <urn:s> <urn:p> ?o } WHERE { ?s ?p ?o VALUES ?duplicate { 1 2 } }",
        &data,
    );
    assert_eq!(rows.len(), data.len());
    assert_eq!(
        rows.into_iter().map(|t| t.object).collect::<HashSet<_>>(),
        values.into_iter().map(Term::from).collect()
    );
}

#[test]
fn construct_internal_dedup_rejects_invalid_positions_and_unbound_values() {
    let rows = results(
        "CONSTRUCT { ?s ?p <urn:o> . <urn:s> <urn:p> ?missing } WHERE { VALUES (?s ?p) { (\"literal\" <urn:p>) (<urn:s> \"literal\") (<urn:s> <urn:p>) (<urn:s> <urn:p>) } }",
        &Dataset::new(),
    );
    assert_eq!(
        rows,
        vec![Triple::new(
            NamedNode::new("urn:s").unwrap(),
            NamedNode::new("urn:p").unwrap(),
            NamedNode::new("urn:o").unwrap()
        )]
    );
}

#[test]
fn construct_internal_dedup_keeps_template_blank_nodes_per_solution() {
    let rows = results(
        "CONSTRUCT { _:fresh <urn:p> ?value . _:fresh <urn:q> ?value } WHERE { VALUES ?value { 1 1 } }",
        &Dataset::new(),
    );
    assert_eq!(rows.len(), 4);
    let subjects = rows
        .iter()
        .map(|t| t.subject.clone())
        .collect::<HashSet<_>>();
    assert_eq!(subjects.len(), 2);
    for subject in subjects {
        assert!(subject.is_blank_node());
        assert_eq!(rows.iter().filter(|t| t.subject == subject).count(), 2);
    }
}

struct RejectNewNodes;

impl<'a> spareval::QueryableDataset<'a> for RejectNewNodes {
    type InternalTerm = Term;
    type Error = std::io::Error;

    fn internal_quads_for_pattern(
        &self,
        _: Option<&Term>,
        _: Option<&Term>,
        _: Option<&Term>,
        _: Option<Option<&Term>>,
    ) -> impl Iterator<Item = Result<spareval::InternalQuad<Term>, Self::Error>> + use<'a> {
        std::iter::empty()
    }

    fn internalize_term(&self, term: Term) -> Result<Term, Self::Error> {
        match term {
            Term::NamedNode(_) | Term::BlankNode(_) => {
                Err(std::io::Error::other("dataset refuses new node identities"))
            }
            term => Ok(term),
        }
    }

    fn externalize_term(&self, term: Term) -> Result<Term, Self::Error> {
        Ok(term)
    }
}

#[test]
fn construct_constants_and_generated_blanks_do_not_require_internalization() {
    let query = spargebra::SparqlParser::new().parse_query("CONSTRUCT { <urn:new> <urn:p> \"constant\" . _:fresh <urn:p> ?value } WHERE { VALUES ?value { 1 1 } }").unwrap();
    let evaluator = QueryEvaluator::new();
    let QueryResults::Graph(rows) = evaluator.prepare(&query).execute(RejectNewNodes).unwrap()
    else {
        panic!("expected graph")
    };
    let rows = rows.map(Result::unwrap).collect::<Vec<_>>();
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter()
            .filter(|row| row.subject.is_blank_node())
            .count(),
        2
    );
}
