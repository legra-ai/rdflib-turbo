//! Stream indexed matches; never retain the set of graph subjects or instances.
use std::collections::HashSet;

use oxigraph::model::*;

use super::backend::Graph;
use super::generalized::{Generalized, Tuples};
use super::{OWL, Pending};
use crate::graph::{RDF, iri, subject};
use crate::{Error, Result};

fn objects<'a>(
    data: &'a Tuples<'_, impl Graph>,
    node: &'a Term,
    predicate: &'a NamedNode,
) -> impl Iterator<Item = Result<Term>> + 'a {
    data.matching([Some(node.as_ref()), Some(predicate.as_ref().into()), None])
        .map(|row| row.map(|row| row[2].clone()))
}

fn unique(data: &Tuples<'_, impl Graph>, node: &Term, predicate: &NamedNode) -> Result<Term> {
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

fn list(
    data: &Tuples<'_, impl Graph>,
    head: &Term,
    invalid_member: &str,
) -> Result<Vec<NamedNode>> {
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
    data: &Tuples<'_, impl Graph>,
    start: &Term,
    node: &Term,
    chain: &[NamedNode],
    property: &NamedNode,
    additions: &mut Pending,
    generalized: &mut Generalized,
) -> Result<()> {
    let Some((step, tail)) = chain.split_first() else {
        return data.push(
            [start.clone(), property.clone().into(), node.clone()],
            additions,
            generalized,
        );
    };
    for value in objects(data, node, step) {
        follow(data, start, &value?, tail, property, additions, generalized)?;
    }
    Ok(())
}

pub(super) fn chains(
    data: &Tuples<'_, impl Graph>,
    additions: &mut Pending,
    generalized: &mut Generalized,
) -> Result<()> {
    for quad in data.matching([
        None,
        Some(iri(OWL, "propertyChainAxiom").as_ref().into()),
        None,
    ]) {
        let quad = quad?;
        let Term::NamedNode(property) = &quad[0] else {
            return Err(Error::Invalid(
                "Property chain predicate must be an IRI".into(),
            ));
        };
        let chain = list(data, &quad[2], "Property chain members must be IRIs")?;
        if chain.is_empty() {
            continue;
        }
        for row in data.matching([None, Some(chain[0].as_ref().into()), None]) {
            let start = row?[0].clone();
            follow(
                data,
                &start,
                &start,
                &chain,
                property,
                additions,
                generalized,
            )?;
        }
    }
    Ok(())
}

fn has_type(data: &Tuples<'_, impl Graph>, node: &Term, class: &Term) -> Result<bool> {
    data.contains(&[node.clone(), iri(RDF, "type").into(), class.clone()])
}

fn shared_key(
    data: &Tuples<'_, impl Graph>,
    left: &Term,
    right: &Term,
    key: &NamedNode,
) -> Result<bool> {
    for value in objects(data, left, key) {
        let value = value?;
        if data.contains(&[right.clone(), key.clone().into(), value])? {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn keys(
    data: &Tuples<'_, impl Graph>,
    additions: &mut Pending,
    generalized: &mut Generalized,
) -> Result<()> {
    for quad in data.matching([None, Some(iri(OWL, "hasKey").as_ref().into()), None]) {
        let quad = quad?;
        let keys = list(data, &quad[2], "Key properties must be IRIs")?;
        let Some(first) = keys.first() else {
            continue;
        };
        let class = &quad[0];
        for member in data.matching([
            None,
            Some(iri(RDF, "type").as_ref().into()),
            Some(class.as_ref()),
        ]) {
            let member = member?[0].clone();
            for value in objects(data, &member, first) {
                let value = value?;
                for candidate in
                    data.matching([None, Some(first.as_ref().into()), Some(value.as_ref())])
                {
                    let candidate = candidate?[0].clone();
                    if candidate == member || !has_type(data, &candidate, class)? {
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
                        data.push(
                            [member.clone(), iri(OWL, "sameAs").into(), candidate],
                            additions,
                            generalized,
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
