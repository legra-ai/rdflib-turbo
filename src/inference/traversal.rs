//! Stream indexed matches; never retain the set of graph subjects or instances.
use std::collections::HashSet;

use oxigraph::model::*;

use super::backend::Graph;
use super::{OWL, Pending};
use crate::graph::{RDF, iri, subject};
use crate::{Error, Result};

fn objects<'a>(
    data: &'a impl Graph,
    node: &'a Term,
    predicate: &'a NamedNode,
) -> impl Iterator<Item = Result<Term>> + 'a {
    let node = match node {
        Term::NamedNode(node) => Some(node.as_ref().into()),
        Term::BlankNode(node) => Some(node.as_ref().into()),
        Term::Literal(_) => None,
    };
    data.quads(node, Some(predicate.as_ref()), None)
        .take(if node.is_some() { usize::MAX } else { 0 })
        .map(|q| q.map(|q| q.object))
}

fn unique(data: &impl Graph, node: &Term, predicate: &NamedNode) -> Result<Term> {
    let mut values = objects(data, node, predicate);
    let first = values
        .next()
        .transpose()?
        .ok_or_else(|| Error::Invalid(format!("Malformed RDF list at {node}")))?;
    for value in values {
        if value? != first {
            return Err(Error::Invalid(format!("Malformed RDF list at {node}")));
        }
    }
    Ok(first)
}

fn list(data: &impl Graph, head: &Term, invalid_member: &str) -> Result<Vec<NamedNode>> {
    let mut node = head.clone();
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    while node != Term::from(iri(RDF, "nil")) {
        subject(&node)?;
        if !seen.insert(node.clone()) {
            return Err(Error::Invalid(format!("Cyclic RDF list at {node}")));
        }
        let first = unique(data, &node, &iri(RDF, "first"))?;
        let next = unique(data, &node, &iri(RDF, "rest"))?;
        let Term::NamedNode(value) = first else {
            return Err(Error::Invalid(invalid_member.into()));
        };
        result.push(value);
        node = next;
    }
    Ok(result)
}

fn follow(
    data: &impl Graph,
    start: &Term,
    node: &Term,
    chain: &[NamedNode],
    property: &NamedNode,
    additions: &mut Pending,
) -> Result<()> {
    let Some((step, tail)) = chain.split_first() else {
        return additions.push(
            data,
            Quad::new(
                subject(start)?,
                property.clone(),
                node.clone(),
                GraphName::DefaultGraph,
            ),
        );
    };
    for value in objects(data, node, step) {
        follow(data, start, &value?, tail, property, additions)?;
    }
    Ok(())
}

pub(super) fn chains(data: &impl Graph, additions: &mut Pending) -> Result<()> {
    for quad in data.quads(None, Some(iri(OWL, "propertyChainAxiom").as_ref()), None) {
        let quad = quad?;
        let NamedOrBlankNode::NamedNode(property) = quad.subject else {
            return Err(Error::Invalid(
                "Property chain predicate must be an IRI".into(),
            ));
        };
        let chain = list(data, &quad.object, "Property chain members must be IRIs")?;
        if chain.is_empty() {
            continue;
        }
        for quad in data.quads(None, Some(chain[0].as_ref()), None) {
            let start: Term = quad?.subject.into();
            follow(data, &start, &start, &chain, &property, additions)?;
        }
    }
    Ok(())
}

fn has_type(data: &impl Graph, node: &Term, class: &Term) -> Result<bool> {
    data.contains(&Quad::new(
        subject(node)?,
        iri(RDF, "type"),
        class.clone(),
        GraphName::DefaultGraph,
    ))
}

fn shared_key(data: &impl Graph, left: &Term, right: &Term, key: &NamedNode) -> Result<bool> {
    for value in objects(data, left, key) {
        let value = value?;
        if data.contains(&Quad::new(
            subject(right)?,
            key.clone(),
            value,
            GraphName::DefaultGraph,
        ))? {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn keys(data: &impl Graph, additions: &mut Pending) -> Result<()> {
    for quad in data.quads(None, Some(iri(OWL, "hasKey").as_ref()), None) {
        let quad = quad?;
        let keys = list(data, &quad.object, "Key properties must be IRIs")?;
        let Some(first) = keys.first() else {
            continue;
        };
        let class: Term = quad.subject.into();
        for member in data.quads(None, Some(iri(RDF, "type").as_ref()), Some(class.as_ref())) {
            let member: Term = member?.subject.into();
            for value in objects(data, &member, first) {
                let value = value?;
                for candidate in data.quads(None, Some(first.as_ref()), Some(value.as_ref())) {
                    let candidate: Term = candidate?.subject.into();
                    if candidate == member || !has_type(data, &candidate, &class)? {
                        continue;
                    }
                    let mut matches = true;
                    for key in &keys[1..] {
                        if !shared_key(data, &member, &candidate, key)? {
                            matches = false;
                            break;
                        }
                    }
                    if matches {
                        additions.push(
                            data,
                            Quad::new(
                                subject(&member)?,
                                iri(OWL, "sameAs"),
                                candidate,
                                GraphName::DefaultGraph,
                            ),
                        )?;
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "traversal_tests.rs"]
mod tests;
