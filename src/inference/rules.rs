//! Fixed OWL 2 RL rule programs and the derived-head policy.

use oxigraph::sparql::PreparedSparqlQuery;

use super::backend::Graph;
use super::pending::Pending;
use super::same_as;
use crate::Result;

pub(super) const OWL: &str = "http://www.w3.org/2002/07/owl#";

pub(super) const PREFIXES: &str = "PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
PREFIX owl: <http://www.w3.org/2002/07/owl#>
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>";

// OWL 2 RL/RDF entailment rules, represented as native CONSTRUCT programs.
// https://www.w3.org/TR/owl2-profiles/#Reasoning_in_OWL_2_RL_and_RDF_Graphs
pub(super) const OWL_RULES: &[(&str, &str)] = &[
    (
        "?s owl:sameAs ?s . ?p owl:sameAs ?p . ?o owl:sameAs ?o",
        "?s ?p ?o",
    ),
    ("?s ?q ?o", "?p owl:sameAs ?q . ?s ?p ?o"),
    (
        "?y owl:sameAs ?z",
        "?r owl:onProperty ?p ; owl:maxCardinality ?n . FILTER(?n = 1) ?x a ?r ; ?p ?y, ?z",
    ),
    (
        "?y owl:sameAs ?z",
        "?r owl:onProperty ?p ; owl:maxQualifiedCardinality ?n ; owl:onClass ?c . FILTER(?n = 1) ?x a ?r ; ?p ?y, ?z . ?y a ?c . ?z a ?c",
    ),
    (
        "?y owl:sameAs ?z",
        "?r owl:onProperty ?p ; owl:maxQualifiedCardinality ?n ; owl:onClass owl:Thing . FILTER(?n = 1) ?x a ?r ; ?p ?y, ?z",
    ),
];

/// The pinned original's derived-head policy is separate from stored data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum HeadDisposition {
    Accepted,
    AlreadyPresent,
    RefusedLiteralPredicate,
}

pub(super) fn apply_rules(
    data: &impl Graph,
    queries: &[PreparedSparqlQuery],
    additions: &mut Pending,
    owl: bool,
) -> Result<()> {
    if owl {
        same_as::apply(data, additions)?;
    }
    for query in queries {
        let triples = data.projected_inference_heads(query.clone())?;
        for triple in triples {
            let triple = triple?;
            additions.push_head(data, triple)?;
        }
    }
    Ok(())
}
