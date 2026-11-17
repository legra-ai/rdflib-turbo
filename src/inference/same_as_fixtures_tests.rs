//! Frozen reference rule programs and closures for equality parity tests.
use oxrdf::GeneralizedDataset;
use std::cell::Cell;
use std::collections::HashSet;

use oxigraph::model::*;
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults, SparqlEvaluator};

use super::super::{Graph, OWL_RULES, PREFIXES, Pending, apply_rules};
use super::measurement::Measured;

pub(super) const ORIGINAL: &[(&str, &str)] = &[
    (
        "?s owl:sameAs ?s . ?p owl:sameAs ?p . ?o owl:sameAs ?o",
        "?s ?p ?o",
    ),
    ("?y owl:sameAs ?x", "?x owl:sameAs ?y"),
    ("?x owl:sameAs ?z", "?x owl:sameAs ?y . ?y owl:sameAs ?z"),
    ("?y ?p ?o", "?x owl:sameAs ?y . ?x ?p ?o"),
    ("?s ?q ?o", "?p owl:sameAs ?q . ?s ?p ?o"),
    ("?s ?p ?y", "?x owl:sameAs ?y . ?s ?p ?x"),
];

pub(super) fn iri(value: &str) -> NamedNode {
    NamedNode::new(format!("urn:{value}")).unwrap()
}
pub(super) fn same() -> NamedNode {
    NamedNode::new("http://www.w3.org/2002/07/owl#sameAs").unwrap()
}
pub(super) fn queries(rules: &[(&str, &str)]) -> Vec<PreparedSparqlQuery> {
    // bounded: the fixed built-in rule inventory.
    rules
        .iter()
        .map(|(template, pattern)| {
            SparqlEvaluator::new()
                .parse_query(&format!(
                    "{PREFIXES} CONSTRUCT {{ {template} }} WHERE {{ {pattern} }}"
                ))
                .unwrap()
        })
        .collect()
}
pub(super) fn reference(
    data: &GeneralizedDataset,
    queries: &[PreparedSparqlQuery],
) -> HashSet<GeneralizedQuad> {
    // bounded: exhaustive fixtures have at most a few terms and their one-round consequences.
    let mut result = HashSet::new();
    for query in queries {
        let QueryResults::Graph(triples) =
            query.clone().on_queryable_dataset(data).execute().unwrap()
        else {
            panic!()
        };
        for triple in triples {
            let t = triple.unwrap();
            let quad =
                GeneralizedQuad::new(t.subject, t.predicate, t.object, GraphName::DefaultGraph);
            if !data.contains(&quad) {
                result.insert(quad);
            }
        }
    }
    let view = Measured {
        data,
        work: Cell::new(0),
    };
    let full = super::super::same_as_reference_tests::equality_rules_one_round(&view).unwrap();
    assert!(
        result.is_subset(&full),
        "strict CONSTRUCT remains an independent control"
    );
    if full
        .iter()
        .all(|q| !q.subject.is_literal() && q.predicate.is_named_node())
    {
        assert_eq!(full, result, "ordinary-domain equality remains exact");
    }
    full
}

pub(super) fn frozen_closure(data: &mut GeneralizedDataset) {
    let mut transaction = data.transaction();
    super::super::axioms::owl(
        &mut transaction,
        false,
        &mut super::super::diagnostics::Diagnostics::new(Vec::new(), 4300),
        &super::super::diagnostic_datatypes::LexicalContext::default(),
    )
    .unwrap();
    // bounded: the fixed former rule inventory, retaining its round semantics.
    let rules: Vec<_> = ORIGINAL
        .iter()
        .chain(
            OWL_RULES
                .iter()
                .filter(|(template, _)| !ORIGINAL.iter().any(|(t, _)| t == template)),
        )
        .copied()
        .collect();
    let mut prepared = queries(&rules);
    // Compare independent equality implementations against the same non-equality
    // semantics; the shared catalogue replaced the former local schema clauses.
    prepared.extend(super::super::shared::CompiledRule::for_mode("owlrl").unwrap());
    loop {
        let mut pending = Pending::default();
        apply_rules(&transaction, &prepared, &mut pending, false).unwrap();
        for quad in super::super::same_as_reference_tests::one_round(&transaction).unwrap() {
            pending.insert(quad);
        }
        super::super::traversal::chains(
            &transaction,
            &mut pending,
            &crate::inference::diagnostics::rdf_list::ListWitnesses::new(4300),
        )
        .unwrap();
        super::super::traversal::keys(
            &transaction,
            &mut pending,
            &crate::inference::diagnostics::rdf_list::ListWitnesses::new(4300),
        )
        .unwrap();
        if pending.is_empty() {
            break;
        }
        for quad in pending {
            Graph::insert(&mut transaction, quad).unwrap();
        }
    }
    transaction.commit();
}
