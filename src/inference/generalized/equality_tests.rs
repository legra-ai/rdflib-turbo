//! Compare the execution partition with unrestricted shared equality clauses.

use std::collections::HashSet;

use oxigraph::model::*;
use oxigraph::sparql::{QueryResults, SparqlEvaluator};

use super::{EqualityRules, Generalized, query};
use crate::inference::shared::CompiledRule;
use crate::inference::{OWL_RULES, PREFIXES, Pending, apply_rules};

#[test]
fn partition_matches_all_eight_shared_clauses_on_every_three_term_equality_graph() {
    // bounded: eight standard clauses and two existing ordinary query templates.
    let shared: Vec<_> = EqualityRules::RULES
        .into_iter()
        .map(|id| {
            CompiledRule::try_new(&id.definition())
                .unwrap()
                .into_query()
        })
        .collect();
    let ordinary_queries: Vec<_> = OWL_RULES[..2]
        .iter()
        .map(|(head, body)| {
            SparqlEvaluator::new()
                .parse_query(&format!(
                    "{PREFIXES} CONSTRUCT {{ {head} }} WHERE {{ {body} }}"
                ))
                .unwrap()
        })
        .collect();
    let equality = EqualityRules::try_new().unwrap();
    let same = NamedNode::new("http://www.w3.org/2002/07/owl#sameAs").unwrap();
    let terms = [
        Term::from(NamedNode::new("urn:a").unwrap()),
        Term::from(BlankNode::new("b").unwrap()),
        Term::from(Literal::new_simple_literal("literal")),
    ];
    for mask in 0..512_u16 {
        // bounded: nine possible equality edges plus three data tuples.
        let mut snapshot = HashSet::new();
        for left in 0..3 {
            for right in 0..3 {
                if mask & (1 << (left * 3 + right)) != 0 {
                    snapshot.insert([
                        terms[left].clone(),
                        same.clone().into(),
                        terms[right].clone(),
                    ]);
                }
            }
        }
        snapshot.insert([
            terms[0].clone(),
            NamedNode::new("urn:p").unwrap().into(),
            terms[2].clone(),
        ]);
        snapshot.insert([
            terms[2].clone(),
            NamedNode::new("urn:q").unwrap().into(),
            terms[1].clone(),
        ]);
        snapshot.insert([terms[0].clone(), terms[1].clone(), terms[2].clone()]);
        let mut data = Dataset::new();
        let mut current = Generalized::default();
        for row in &snapshot {
            match (
                NamedOrBlankNode::try_from(row[0].clone()),
                NamedNode::try_from(row[1].clone()),
            ) {
                (Ok(subject), Ok(predicate)) => {
                    data.insert(&Quad::new(
                        subject,
                        predicate,
                        row[2].clone(),
                        GraphName::DefaultGraph,
                    ));
                }
                _ => current.insert(row.clone()),
            }
        }
        let journal = data.insert_transaction();
        // bounded: outputs over the twelve-tuple fixture and fixed vocabulary.
        let mut expected = HashSet::new();
        for rule in &shared {
            let QueryResults::Solutions(rows) = rule
                .clone()
                .on_queryable_dataset(query::View {
                    data: &journal,
                    extra: &current,
                })
                .execute()
                .unwrap()
            else {
                panic!("expected unrestricted shared head bindings")
            };
            for row in rows {
                let row = row.unwrap();
                let head =
                    ["__head_s", "__head_p", "__head_o"].map(|name| row.get(name).unwrap().clone());
                if !snapshot.contains(&head) {
                    expected.insert(head);
                }
            }
        }
        let mut ordinary = Pending::default();
        let mut pending = Generalized::default();
        equality
            .apply(&current, &journal, &mut ordinary, &mut pending)
            .unwrap();
        apply_rules(&journal, &ordinary_queries, &mut ordinary, true).unwrap();
        // bounded: only distinct one-round outputs over this finite fixture.
        let actual: HashSet<_> = ordinary
            .0
            .into_iter()
            .map(|quad| [quad.subject.into(), quad.predicate.into(), quad.object])
            .chain(
                pending
                    .matching([None, None, None])
                    .map(|row| row.map(TermRef::into_owned)),
            )
            .collect();
        assert_eq!(
            actual, expected,
            "equality partition differs for graph {mask:09b}"
        );
    }
}

#[test]
fn new_literal_inverse_is_not_visible_until_the_next_round() {
    let same = NamedNode::new("http://www.w3.org/2002/07/owl#sameAs").unwrap();
    let a = NamedNode::new("urn:a").unwrap();
    let b = NamedNode::new("urn:b").unwrap();
    let literal = Literal::new_simple_literal("shared");
    let mut data = Dataset::new();
    for subject in [&a, &b] {
        data.insert(&Quad::new(
            subject.clone(),
            same.clone(),
            literal.clone(),
            GraphName::DefaultGraph,
        ));
    }
    let journal = data.insert_transaction();
    let equality = EqualityRules::try_new().unwrap();
    let mut current = Generalized::default();
    let mut pending = Generalized::default();
    let mut ordinary = Pending::default();
    equality
        .apply(&current, &journal, &mut ordinary, &mut pending)
        .unwrap();
    let consequence = Quad::new(a, same.clone(), b, GraphName::DefaultGraph);
    assert!(!ordinary.0.contains(&consequence));
    assert!(current.rows.is_empty());
    assert!(!pending.rows.is_empty());
    current.extend(pending);
    equality
        .apply(
            &current,
            &journal,
            &mut ordinary,
            &mut Generalized::default(),
        )
        .unwrap();
    assert!(ordinary.0.contains(&consequence));
}

#[test]
fn object_replacement_joins_exact_numeric_lexical_identities() {
    let same = NamedNode::new("http://www.w3.org/2002/07/owl#sameAs").unwrap();
    let datatype = NamedNode::new("http://www.w3.org/2001/XMLSchema#integer").unwrap();
    let equality = EqualityRules::try_new().unwrap();
    let subject = NamedNode::new("urn:s").unwrap();
    let predicate = NamedNode::new("urn:p").unwrap();
    let alias = NamedNode::new("urn:alias").unwrap();
    for lexical in ["01", "1"] {
        let mut data = Dataset::new();
        data.insert(&Quad::new(
            subject.clone(),
            predicate.clone(),
            Literal::new_typed_literal(lexical, datatype.clone()),
            GraphName::DefaultGraph,
        ));
        let journal = data.insert_transaction();
        let mut current = Generalized::default();
        current.insert([
            Literal::new_typed_literal("01", datatype.clone()).into(),
            same.clone().into(),
            alias.clone().into(),
        ]);
        let mut ordinary = Pending::default();
        equality
            .apply(
                &current,
                &journal,
                &mut ordinary,
                &mut Generalized::default(),
            )
            .unwrap();
        assert_eq!(
            ordinary.0.contains(&Quad::new(
                subject.clone(),
                predicate.clone(),
                alias.clone(),
                GraphName::DefaultGraph
            )),
            lexical == "01"
        );
    }
}
