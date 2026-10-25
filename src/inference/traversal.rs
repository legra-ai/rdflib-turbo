//! Stream full-Term indexed matches without retaining graph subjects or instances.
use oxigraph::model::*;

use super::backend::Graph;
use super::diagnostics::rdf_list::{ListWitnesses, RdfList};
use super::{OWL, Pending};
use crate::Result;
use crate::graph::{RDF, iri};

fn objects<'a>(
    data: &'a impl Graph,
    node: &'a Term,
    predicate: TermRef<'a>,
) -> impl Iterator<Item = Result<Term>> + 'a {
    data.quads(Some(node.as_ref()), Some(predicate), None)
        .map(|q| q.map(|q| q.object))
}

fn list(data: &impl Graph, head: &Term, witnesses: &ListWitnesses) -> Result<Vec<Term>> {
    let list = RdfList::new(data, witnesses, head.clone())?;
    let mut result = Vec::with_capacity(list.len);
    let mut cursor = list.cursor();
    while let Some(member) = cursor.next()? {
        result.push(member);
    }
    Ok(result)
}

fn follow(
    data: &impl Graph,
    start: &Term,
    node: &Term,
    chain: &[Term],
    property: &Term,
    additions: &mut Pending,
) -> Result<()> {
    let Some((step, tail)) = chain.split_first() else {
        return additions
            .push(
                data,
                GeneralizedQuad::new(
                    start.clone(),
                    property.clone(),
                    node.clone(),
                    GraphName::DefaultGraph,
                ),
            )
            .map(|_| ());
    };
    for value in objects(data, node, step.as_ref()) {
        follow(data, start, &value?, tail, property, additions)?;
    }
    Ok(())
}

pub(super) fn chains(
    data: &impl Graph,
    additions: &mut Pending,
    witnesses: &ListWitnesses,
) -> Result<()> {
    for quad in data.quads(
        None,
        Some(iri(OWL, "propertyChainAxiom").as_ref().into()),
        None,
    ) {
        let quad = quad?;
        let property = quad.subject;
        let chain = list(data, &quad.object, witnesses)?;
        if chain.is_empty() {
            continue;
        }
        for quad in data.quads(None, Some(chain[0].as_ref()), None) {
            let start = quad?.subject;
            follow(data, &start, &start, &chain, &property, additions)?;
        }
    }
    Ok(())
}

fn has_type(data: &impl Graph, node: &Term, class: &Term) -> Result<bool> {
    data.contains(GeneralizedQuadRef {
        subject: node.as_ref(),
        predicate: iri(RDF, "type").as_ref().into(),
        object: class.as_ref(),
        graph_name: GraphNameRef::DefaultGraph,
    })
}

fn shared_key(data: &impl Graph, left: &Term, right: &Term, key: &Term) -> Result<bool> {
    for value in objects(data, left, key.as_ref()) {
        let value = value?;
        if data.contains(GeneralizedQuadRef {
            subject: right.as_ref(),
            predicate: key.as_ref(),
            object: value.as_ref(),
            graph_name: GraphNameRef::DefaultGraph,
        })? {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn keys(
    data: &impl Graph,
    additions: &mut Pending,
    witnesses: &ListWitnesses,
) -> Result<()> {
    for quad in data.quads(None, Some(iri(OWL, "hasKey").as_ref().into()), None) {
        let quad = quad?;
        let keys = list(data, &quad.object, witnesses)?;
        let Some(first) = keys.first() else {
            continue;
        };
        let class = quad.subject;
        for member in data.quads(
            None,
            Some(iri(RDF, "type").as_ref().into()),
            Some(class.as_ref()),
        ) {
            let member = member?.subject;
            for value in objects(data, &member, first.as_ref()) {
                let value = value?;
                for candidate in data.quads(None, Some(first.as_ref()), Some(value.as_ref())) {
                    let candidate = candidate?.subject;
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
                            GeneralizedQuad::new(
                                member.clone(),
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
