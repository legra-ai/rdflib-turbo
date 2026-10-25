use std::collections::HashSet;

use oxigraph::model::*;
use oxigraph::sparql::{QueryResults, SparqlEvaluator};

use crate::graph::{RDF, iri, objects, subject, subjects};
use crate::path::rdf_list;
use crate::{Error, Result};

#[cfg(test)]
#[path = "inference_tests.rs"]
mod tests;

mod axioms;

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
    ("?y owl:sameAs ?x", "?x owl:sameAs ?y"),
    ("?x owl:sameAs ?z", "?x owl:sameAs ?y . ?y owl:sameAs ?z"),
    ("?y ?p ?o", "?x owl:sameAs ?y . ?x ?p ?o"),
    ("?s ?q ?o", "?p owl:sameAs ?q . ?s ?p ?o"),
    ("?s ?p ?y", "?x owl:sameAs ?y . ?s ?p ?x"),
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
        "?c owl:intersectionOf ?list . ?x a ?any FILTER NOT EXISTS { ?list rdf:rest*/rdf:first ?member FILTER NOT EXISTS { ?x a ?member } }",
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

pub fn expand(data: &mut Dataset, mode: &str) -> Result<()> {
    if data.iter().any(|q| !q.graph_name.is_default_graph()) {
        let before = crate::graph::with_union(data);
        let mut working = before.clone();
        expand_inner(&mut working, mode)?;
        crate::graph::commit_additions(data, &before, &working);
        return Ok(());
    }
    expand_inner(data, mode)
}

/// Infer over the dataset union and put only new statements in the destination.
/// Original graph contexts remain available to callers and report descriptions.
pub fn expand_into(data: &mut Dataset, mode: &str, destination: GraphName) -> Result<()> {
    let before = crate::graph::with_union(data);
    let mut working = before.clone();
    expand_inner(&mut working, mode)?;
    data.extend(
        working
            .iter()
            .filter(|q| !before.contains(*q))
            .map(|q| Quad::new(q.subject, q.predicate, q.object, destination.clone())),
    );
    Ok(())
}

fn expand_inner(data: &mut Dataset, mode: &str) -> Result<()> {
    match mode {
        "none" | "" => return Ok(()),
        "rdfs" | "owlrl" | "both" => {}
        _ => return Err(Error::Unsupported(format!("inference: {mode}"))),
    }
    let mut rules = Vec::new();
    if mode != "owlrl" {
        rules.extend_from_slice(RDFS_RULES);
    }
    if mode != "rdfs" {
        rules.extend_from_slice(OWL_RULES);
        axioms::owl(data, mode == "both");
    }
    let mut initial_resources = if mode != "owlrl" {
        axioms::resources(data)
    } else {
        Vec::new()
    };
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
        let mut additions = std::mem::take(&mut initial_resources);
        for query in &queries {
            let QueryResults::Graph(triples) =
                query.clone().on_queryable_dataset(&*data).execute()?
            else {
                unreachable!("Inference rules are CONSTRUCT queries")
            };
            for triple in triples {
                let triple = triple?;
                additions.push(Quad::new(
                    triple.subject,
                    triple.predicate,
                    triple.object,
                    GraphName::DefaultGraph,
                ));
            }
        }
        if mode != "rdfs" {
            // Lists can themselves be inferred, so discover them each round.
            for quad in data
                .iter()
                .filter(|q| q.predicate == iri(OWL, "propertyChainAxiom").as_ref())
            {
                let Term::NamedNode(property) = Term::from(quad.subject.into_owned()) else {
                    return Err(Error::Invalid(
                        "Property chain predicate must be an IRI".into(),
                    ));
                };
                let chain = rdf_list(data, &quad.object.into_owned())?;
                if chain.is_empty() {
                    continue;
                }
                let starts: Vec<Term> =
                    data.iter().map(|q| q.subject.into_owned().into()).collect();
                for start in starts {
                    let mut current = vec![start.clone()];
                    for step in &chain {
                        let Term::NamedNode(step) = step else {
                            return Err(Error::Invalid(
                                "Property chain members must be IRIs".into(),
                            ));
                        };
                        current = current
                            .iter()
                            .map(|n| objects(data, n, step))
                            .collect::<Result<Vec<_>>>()?
                            .into_iter()
                            .flatten()
                            .collect();
                    }
                    for end in current {
                        additions.push(Quad::new(
                            crate::graph::subject(&start)?,
                            property.clone(),
                            end,
                            GraphName::DefaultGraph,
                        ));
                    }
                }
            }
            for quad in data
                .iter()
                .filter(|q| q.predicate == iri(OWL, "hasKey").as_ref())
            {
                let keys = rdf_list(data, &quad.object.into_owned())?;
                // The reference reasoner treats an empty key as inactive.
                if keys.is_empty() {
                    continue;
                }
                let keys = keys
                    .into_iter()
                    .map(|key| match key {
                        Term::NamedNode(key) => Ok(key),
                        _ => Err(Error::Invalid("Key properties must be IRIs".into())),
                    })
                    .collect::<Result<Vec<_>>>()?;
                let class = quad.subject.into_owned().into();
                let members: HashSet<_> = subjects(data, &iri(RDF, "type"), Some(&class))?
                    .into_iter()
                    .collect();
                for member in &members {
                    let mut candidates: Option<HashSet<Term>> = None;
                    for key in &keys {
                        // Each key must share at least one exact RDF value.
                        // Query indexes with terms directly: interpolating a
                        // blank-node class into SPARQL makes it a variable.
                        let mut matches = HashSet::new();
                        for value in objects(data, member, key)? {
                            matches.extend(
                                subjects(data, key, Some(&value))?
                                    .into_iter()
                                    .filter(|n| n != member && members.contains(n)),
                            );
                        }
                        if let Some(candidates) = &mut candidates {
                            candidates.retain(|n| matches.contains(n));
                        } else {
                            candidates = Some(matches);
                        }
                        if candidates.as_ref().is_some_and(HashSet::is_empty) {
                            break;
                        }
                    }
                    for other in candidates.into_iter().flatten() {
                        additions.push(Quad::new(
                            subject(member)?,
                            iri(OWL, "sameAs"),
                            other,
                            GraphName::DefaultGraph,
                        ));
                    }
                }
            }
        }
        let before = data.len();
        data.extend(additions);
        if data.len() == before {
            return Ok(());
        }
    }
}
