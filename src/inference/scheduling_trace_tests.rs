//! Fixed catalogue ordinals are checked against grouped bodies and actual heads.
use std::collections::HashSet;

use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::store::Store;
use rdf_reasoning::{RuleId, RuleId::*};

use super::{CompiledRule, Schedule};

const RDFS_CLAUSES: &[&[RuleId]] = &[
    &[Rdf1],
    &[Rdfs2],
    &[Rdfs3],
    &[Rdfs5],
    &[Rdfs6],
    &[Rdfs7],
    &[Rdfs8, Rdfs10],
    &[Rdfs9],
    &[Rdfs11],
    &[Rdfs12],
    &[Rdfs13],
];

#[test]
fn rdfs_trace_ordinals_match_catalogue_bodies_and_nonempty_native_query_heads() {
    let selected = [
        Rdf1, Rdfs2, Rdfs3, Rdfs5, Rdfs6, Rdfs7, Rdfs8, Rdfs9, Rdfs10, Rdfs11, Rdfs12, Rdfs13,
    ];
    // bounded: the twelve fixed published RDFS definitions, never caller facts.
    let mut groups: Vec<Vec<RuleId>> = Vec::new();
    for id in selected {
        if let Some(group) = groups
            .iter_mut()
            .find(|group| group[0].definition().body == id.definition().body)
        {
            group.push(id);
        } else {
            groups.push(vec![id]);
        }
    }
    // bounded: one entry per fixed catalogue body group.
    let grouped: Vec<_> = groups.iter().map(Vec::as_slice).collect();
    assert_eq!(grouped, RDFS_CLAUSES);
    let schedule = Schedule::for_mode("rdfs").unwrap();
    assert_eq!(schedule.clauses.len(), RDFS_CLAUSES.len());
    let store = Store::new().unwrap();
    let input = r#"
        @prefix ex: <urn:trace:> .
        @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
        @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
        ex:p rdfs:domain ex:C; rdfs:range ex:D; rdfs:subPropertyOf ex:r; a rdf:Property .
        ex:q rdfs:subPropertyOf ex:p .
        ex:x ex:p ex:y; ex:q ex:y; a ex:C .
        ex:C a rdfs:Class; rdfs:subClassOf ex:D .
        ex:D rdfs:subClassOf ex:E .
        ex:container a rdfs:ContainerMembershipProperty .
        ex:datatype a rdfs:Datatype .
    "#;
    for quad in RdfParser::from_format(RdfFormat::Turtle).for_reader(input.as_bytes()) {
        store.insert(quad.unwrap().as_ref()).unwrap();
    }
    for (ordinal, ids) in RDFS_CLAUSES.iter().enumerate() {
        // bounded: at most two static definitions in this catalogue group.
        let definitions: Vec<_> = ids.iter().map(|id| id.definition()).collect();
        let mut expected_queries = CompiledRule::try_catalogue(&definitions).unwrap();
        assert_eq!(expected_queries.len(), 1);
        let expected_query = expected_queries.pop().unwrap();
        // bounded: complete heads over the sixteen-row fixed mapping witness.
        let expected: HashSet<_> = expected_query
            .on_store(&store)
            .execute_inference_construct()
            .unwrap()
            .map(|head| head.unwrap())
            .collect();
        // bounded: same fixed witness; full RDF terms are compared without filtering.
        let actual: HashSet<_> = schedule.clauses[ordinal]
            .query
            .clone()
            .on_store(&store)
            .execute_inference_construct()
            .unwrap()
            .map(|head| head.unwrap())
            .collect();
        assert!(
            !expected.is_empty(),
            "catalogue witness is empty for {ids:?}"
        );
        assert_eq!(
            actual, expected,
            "compiled catalogue ordinal {ordinal}: {ids:?}"
        );
    }
    // bounded: one printable identifier per fixed clause, outside allocation measurements.
    let mapping: Vec<_> = RDFS_CLAUSES
        .iter()
        .enumerate()
        .map(|(ordinal, ids)| (ordinal, format!("{ids:?}")))
        .collect();
    println!(
        "MEMORY_CATALOGUE {}",
        serde_json::to_string(&mapping).unwrap()
    );
}
