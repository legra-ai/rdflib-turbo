use std::collections::HashSet;

use oxigraph::model::*;
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults, SparqlEvaluator};

use crate::{Error, Result};

mod axioms;
mod backend;
mod borrowed;
pub(crate) use borrowed::BorrowedInference;
pub(crate) use diagnostic_datatypes::SignalPublisher;
pub(crate) use outcome::bind_error_namespace;
mod delta;
mod diagnostic_datatypes;
mod diagnostics;
mod intersections;
mod literal_values;
mod options;
mod outcome;
mod request;
mod same_as;
pub(crate) mod scoped;
mod shared;
#[cfg(test)]
mod shared_rule_tests;
mod staged;
#[cfg(test)]
mod staged_tests;
mod traversal;

pub(crate) use delta::expand_delta;
pub(crate) use diagnostic_datatypes::LexicalFailure;
pub(crate) use options::InferenceOptions;
use options::expand_configured;
pub(crate) use outcome::InferenceOutcome;
pub(crate) use request::InferenceRequest;

#[cfg(test)]
mod diagnostic_tests;
#[cfg(test)]
mod failure_tests;
#[cfg(test)]
mod intersection_tests;
#[cfg(test)]
mod literal_value_tests;
#[cfg(test)]
mod same_as_tests;

use backend::Graph;

const OWL: &str = "http://www.w3.org/2002/07/owl#";

const PREFIXES: &str = "PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
PREFIX owl: <http://www.w3.org/2002/07/owl#>
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>";

// OWL 2 RL/RDF entailment rules, represented as native CONSTRUCT programs.
// https://www.w3.org/TR/owl2-profiles/#Reasoning_in_OWL_2_RL_and_RDF_Graphs
const OWL_RULES: &[(&str, &str)] = &[
    (
        "?s owl:sameAs ?s . ?p owl:sameAs ?p . ?o owl:sameAs ?o",
        "?s ?p ?o",
    ),
    ("?s ?q ?o", "?p owl:sameAs ?q . ?s ?p ?o"),
    (
        "?x a ?member",
        "?c owl:intersectionOf ?list . ?list rdf:rest*/rdf:first ?member . ?x a ?c",
    ),
    (
        "?x a ?c",
        "?c owl:unionOf ?list . ?list rdf:rest*/rdf:first ?member . ?x a ?member",
    ),
    (
        "?x a ?c",
        "?c owl:oneOf ?list . ?list rdf:rest*/rdf:first ?x",
    ),
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
    (
        "?c rdfs:subClassOf ?member",
        "?c owl:intersectionOf ?list . ?list rdf:rest*/rdf:first ?member",
    ),
    (
        "?member rdfs:subClassOf ?c",
        "?c owl:unionOf ?list . ?list rdf:rest*/rdf:first ?member",
    ),
];

pub(crate) fn expand(
    data: &mut Dataset,
    mode: &str,
    request: InferenceRequest,
) -> Result<InferenceOutcome> {
    if data.iter().any(|q| !q.graph_name.is_default_graph()) {
        return expand_into(data, mode, GraphName::DefaultGraph, request);
    }
    let mut transaction = data.insert_transaction();
    let (_, outcome) = expand_configured(&mut transaction, mode, request)?;
    transaction.commit();
    Ok(outcome)
}

/// Infer over the dataset union and put output in the exact destination.
/// Original graph contexts remain available to callers and report descriptions.
pub(crate) fn expand_into(
    data: &mut Dataset,
    mode: &str,
    destination: GraphName,
    request: InferenceRequest,
) -> Result<InferenceOutcome> {
    staged::expand(data, mode, destination, request)
}

/// One round retains only distinct statements absent from its read snapshot.
/// Existing facts and duplicate rule results never enter the retained buffer.
#[derive(Default)]
struct Pending(HashSet<Quad>);

impl Pending {
    fn push(&mut self, data: &impl Graph, quad: Quad) -> Result<()> {
        if !data.contains(&quad)? {
            self.0.insert(quad);
        }
        Ok(())
    }
}

fn apply_rules(
    data: &impl Graph,
    queries: &[PreparedSparqlQuery],
    additions: &mut Pending,
    owl: bool,
) -> Result<()> {
    if owl {
        same_as::apply(data, additions)?;
    }
    for query in queries {
        let QueryResults::Graph(triples) = data.query(query.clone())? else {
            unreachable!("Inference rules are CONSTRUCT queries")
        };
        for triple in triples {
            let triple = triple?;
            additions.push(
                data,
                Quad::new(
                    triple.subject,
                    triple.predicate,
                    triple.object,
                    GraphName::DefaultGraph,
                ),
            )?;
        }
    }
    Ok(())
}

fn expand_inner(
    data: &mut impl Graph,
    mode: &str,
    request: InferenceRequest,
) -> Result<InferenceOutcome> {
    match mode {
        "none" | "" => return Ok(InferenceOutcome::default()),
        "rdfs" | "owlrl" | "both" => {}
        _ => return Err(Error::Unsupported(format!("inference: {mode}"))),
    }
    if mode == "rdfs" {
        // RDFS_Semantics.one_time_rules publishes every literal's identity pair
        // directly to its destination, even when the input contains that SPO.
        // Retain only the output delta; no source snapshot is needed.
        let mut literal_publications = Pending::default();
        for quad in data.quads(None, None, None) {
            let mut quad = quad?;
            if quad.object.is_literal() && !data.contains_destination(&quad)? {
                quad.graph_name = GraphName::DefaultGraph;
                literal_publications.0.insert(quad);
            }
        }
        for quad in literal_publications.0 {
            data.insert(quad)?;
        }
        literal_values::strings(data)?;
    }
    let mut diagnostics = diagnostics::Diagnostics::new(request.prior_error_messages);
    // bounded: the fixed RDFS/OWL rule tables above.
    let mut rules = Vec::new();
    if mode != "rdfs" {
        rules.extend_from_slice(OWL_RULES);
        axioms::owl(
            data,
            mode == "both",
            &mut diagnostics,
            &request.lexical_context,
        )?;
    }
    let mut initial_resources = Pending::default();
    if mode != "owlrl" {
        for quad in axioms::resources(data) {
            initial_resources.push(data, quad?)?;
        }
    }
    // bounded: one prepared query per built-in rule.
    let mut queries = shared::CompiledRule::for_mode(mode)?;
    queries.extend(
        rules
            .iter()
            .map(|(template, pattern)| {
                SparqlEvaluator::new()
                    .parse_query(&format!(
                        "{PREFIXES}\nCONSTRUCT {{ {template} }} WHERE {{ {pattern} }}"
                    ))
                    .map_err(|e| Error::Runtime(format!("Invalid inference rule: {e}")))
            })
            .collect::<Result<Vec<_>>>()?,
    );
    loop {
        if mode != "rdfs" {
            diagnostics.round(data)?;
        }
        let mut additions = std::mem::take(&mut initial_resources);
        apply_rules(data, &queries, &mut additions, mode != "rdfs")?;
        if mode != "rdfs" {
            // Lists can themselves be inferred, so discover them each round.
            intersections::additions(data, &mut additions)?;
            traversal::chains(data, &mut additions)?;
            traversal::keys(data, &mut additions)?;
        }
        if additions.0.is_empty() {
            return diagnostics.finish(data, mode != "rdfs", |term| {
                diagnostics::cardinality_zero(term, &request.lexical_context)
            });
        }
        for quad in additions.0 {
            data.insert(quad)?;
        }
    }
}

/// Expand directly over the active transaction's union, without copying its base.
pub(crate) fn expand_transaction(
    transaction: &mut oxigraph::store::Transaction<'_>,
    mode: &str,
    destination: GraphName,
    request: InferenceRequest,
) -> Result<InferenceOutcome> {
    expand_configured(
        &mut backend::TransactionGraph {
            transaction,
            destination,
        },
        mode,
        request,
    )
    .map(|(_, outcome)| outcome)
}

#[cfg(test)]
mod insertion_failure_tests;

#[cfg(test)]
mod borrowed_tests;
