use std::collections::HashSet;

use oxigraph::model::*;
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults, SparqlEvaluator};

use crate::{Error, Result};

mod axioms;
mod backend;
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
mod traversal;

pub(crate) use delta::expand_delta;
pub(crate) use diagnostic_datatypes::LexicalFailure;
pub(crate) use options::InferenceOptions;
use options::PreparedOptions;
pub(crate) use outcome::InferenceOutcome;
pub(crate) use request::InferenceRequest;

#[cfg(test)]
mod failure_tests;
#[cfg(test)]
mod diagnostic_tests;
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

const RDFS_RULES: &[(&str, &str)] = &[
    ("?p a rdf:Property", "?s ?p ?o"),
    ("?s a ?c", "?p rdfs:domain ?c . ?s ?p ?o"),
    ("?o a ?c", "?p rdfs:range ?c . ?s ?p ?o"),
    (
        "?p rdfs:subPropertyOf ?r",
        "?p rdfs:subPropertyOf ?q . ?q rdfs:subPropertyOf ?r",
    ),
    ("?p rdfs:subPropertyOf ?p", "?p a rdf:Property"),
    ("?s ?q ?o", "?p rdfs:subPropertyOf ?q . ?s ?p ?o"),
    ("?c rdfs:subClassOf rdfs:Resource, ?c", "?c a rdfs:Class"),
    ("?s a ?d", "?c rdfs:subClassOf ?d . ?s a ?c"),
    (
        "?a rdfs:subClassOf ?c",
        "?a rdfs:subClassOf ?b . ?b rdfs:subClassOf ?c",
    ),
    (
        "?p rdfs:subPropertyOf rdfs:member",
        "?p a rdfs:ContainerMembershipProperty",
    ),
    ("?d rdfs:subClassOf rdfs:Literal", "?d a rdfs:Datatype"),
];

// OWL 2 RL/RDF entailment rules, represented as native CONSTRUCT programs.
// https://www.w3.org/TR/owl2-profiles/#Reasoning_in_OWL_2_RL_and_RDF_Graphs
const OWL_RULES: &[(&str, &str)] = &[
    (
        "?s owl:sameAs ?s . ?p owl:sameAs ?p . ?o owl:sameAs ?o",
        "?s ?p ?o",
    ),
    ("?s ?q ?o", "?p owl:sameAs ?q . ?s ?p ?o"),
    ("?s a ?c", "?p rdfs:domain ?c . ?s ?p ?o"),
    ("?o a ?c", "?p rdfs:range ?c . ?s ?p ?o"),
    (
        "?y owl:sameAs ?z",
        "?p a owl:FunctionalProperty . ?x ?p ?y, ?z",
    ),
    (
        "?x owl:sameAs ?y",
        "?p a owl:InverseFunctionalProperty . ?x ?p ?z . ?y ?p ?z",
    ),
    ("?y ?p ?x", "?p a owl:SymmetricProperty . ?x ?p ?y"),
    (
        "?x ?p ?z",
        "?p a owl:TransitiveProperty . ?x ?p ?y . ?y ?p ?z",
    ),
    ("?x ?q ?y", "?p rdfs:subPropertyOf ?q . ?x ?p ?y"),
    (
        "?p rdfs:subPropertyOf ?q . ?q rdfs:subPropertyOf ?p",
        "?p owl:equivalentProperty ?q FILTER(!sameTerm(?p, ?q))",
    ),
    ("?y ?q ?x", "?p owl:inverseOf ?q . ?x ?p ?y"),
    ("?y ?p ?x", "?p owl:inverseOf ?q . ?x ?q ?y"),
    ("?x a ?d", "?c rdfs:subClassOf ?d . ?x a ?c"),
    (
        "?c rdfs:subClassOf ?d . ?d rdfs:subClassOf ?c",
        "?c owl:equivalentClass ?d FILTER(!sameTerm(?c, ?d))",
    ),
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
        "?x a ?r",
        "?r owl:onProperty ?p ; owl:someValuesFrom ?c . ?x ?p ?y . ?y a ?c",
    ),
    (
        "?x a ?r",
        "?r owl:onProperty ?p ; owl:someValuesFrom owl:Thing . ?x ?p ?y",
    ),
    (
        "?y a ?c",
        "?r owl:onProperty ?p ; owl:allValuesFrom ?c . ?x a ?r ; ?p ?y",
    ),
    (
        "?x ?p ?v",
        "?r owl:onProperty ?p ; owl:hasValue ?v . ?x a ?r",
    ),
    (
        "?x a ?r",
        "?r owl:onProperty ?p ; owl:hasValue ?v . ?x ?p ?v",
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
        "?c rdfs:subClassOf ?c, owl:Thing ; owl:equivalentClass ?c . owl:Nothing rdfs:subClassOf ?c",
        "?c a owl:Class",
    ),
    // The pinned reference emits reflexive schema entailments through class /
    // property declarations, not through cycles involving undeclared terms.
    (
        "?a rdfs:subClassOf ?c",
        "?a rdfs:subClassOf ?b . ?b rdfs:subClassOf ?c FILTER(!sameTerm(?a, ?c))",
    ),
    (
        "?a owl:equivalentClass ?b",
        "?a rdfs:subClassOf ?b . ?b rdfs:subClassOf ?a",
    ),
    (
        "?p rdfs:subPropertyOf ?p ; owl:equivalentProperty ?p",
        "{ ?p a owl:ObjectProperty } UNION { ?p a owl:DatatypeProperty } UNION { ?p a rdf:Property }",
    ),
    (
        "?a rdfs:subPropertyOf ?c",
        "?a rdfs:subPropertyOf ?b . ?b rdfs:subPropertyOf ?c FILTER(!sameTerm(?a, ?c))",
    ),
    (
        "?a owl:equivalentProperty ?b",
        "?a rdfs:subPropertyOf ?b . ?b rdfs:subPropertyOf ?a FILTER(!sameTerm(?a, ?b))",
    ),
    (
        "?p rdfs:domain ?d",
        "?p rdfs:domain ?c . ?c rdfs:subClassOf ?d",
    ),
    (
        "?p rdfs:domain ?c",
        "?p rdfs:subPropertyOf ?q . ?q rdfs:domain ?c",
    ),
    (
        "?p rdfs:range ?d",
        "?p rdfs:range ?c . ?c rdfs:subClassOf ?d",
    ),
    (
        "?p rdfs:range ?c",
        "?p rdfs:subPropertyOf ?q . ?q rdfs:range ?c",
    ),
    (
        "?r rdfs:subClassOf ?s",
        "?r owl:hasValue ?v ; owl:onProperty ?p . ?s owl:hasValue ?v ; owl:onProperty ?q . ?p rdfs:subPropertyOf ?q",
    ),
    (
        "?r rdfs:subClassOf ?s",
        "?r owl:someValuesFrom ?c ; owl:onProperty ?p . ?s owl:someValuesFrom ?d ; owl:onProperty ?p . ?c rdfs:subClassOf ?d",
    ),
    (
        "?r rdfs:subClassOf ?s",
        "?r owl:someValuesFrom ?c ; owl:onProperty ?p . ?s owl:someValuesFrom ?c ; owl:onProperty ?q . ?p rdfs:subPropertyOf ?q",
    ),
    (
        "?r rdfs:subClassOf ?s",
        "?r owl:allValuesFrom ?c ; owl:onProperty ?p . ?s owl:allValuesFrom ?d ; owl:onProperty ?p . ?c rdfs:subClassOf ?d",
    ),
    (
        "?r rdfs:subClassOf ?s",
        "?r owl:allValuesFrom ?c ; owl:onProperty ?p . ?s owl:allValuesFrom ?c ; owl:onProperty ?q . ?q rdfs:subPropertyOf ?p",
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
    let mut union = crate::graph::with_union(data);
    let mut transaction = union.insert_transaction();
    let (prepared, outcome) = expand_configured(&mut transaction, mode, request)?;
    let publications = delta::literal_publications(data, mode, &destination);
    let mut destination_transaction = data.insert_transaction();
    for quad in publications.iter() {
        destination_transaction.insert(quad);
    }
    for quad in transaction.inserted() {
        destination_transaction.insert(&Quad::new(quad.subject, quad.predicate, quad.object, destination.clone()));
    }
    prepared.insert(|mut quad| { quad.graph_name = destination.clone(); destination_transaction.insert(&quad); })?;
    destination_transaction.commit();
    transaction.commit();
    Ok(outcome)
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

fn expand_configured<'a>(data: &mut impl Graph, mode: &'a str, request: InferenceRequest) -> Result<(PreparedOptions<'a>, InferenceOutcome)> {
    match mode {
        "none" | "" => return Ok((PreparedOptions::new(data, mode, InferenceOptions::default())?, InferenceOutcome::default())),
        "rdfs" | "owlrl" | "both" => {},
        _ => return Err(Error::Unsupported(format!("inference: {mode}"))),
    }
    let prepared = PreparedOptions::new(data, mode, request.options)?;
    prepared.insert(|quad| data.insert(quad))?;
    let outcome = expand_inner(data, mode, request)?;
    Ok((prepared, outcome))
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
            data.insert(quad);
        }
        literal_values::strings(data)?;
    }
    let mut diagnostics = diagnostics::Diagnostics::new(request.prior_error_messages);
    // bounded: the fixed RDFS/OWL rule tables above.
    let mut rules = Vec::new();
    if mode != "owlrl" {
        rules.extend_from_slice(RDFS_RULES);
    }
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
    let queries = rules
        .iter()
        .map(|(template, pattern)| {
            SparqlEvaluator::new()
                .parse_query(&format!(
                    "{PREFIXES}\nCONSTRUCT {{ {template} }} WHERE {{ {pattern} }}"
                ))
                .map_err(|e| Error::Runtime(format!("Invalid inference rule: {e}")))
        })
        .collect::<Result<Vec<_>>>()?;
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
            data.insert(quad);
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
