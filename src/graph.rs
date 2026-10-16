use crate::{Error, Result};
use oxigraph::model::*;
pub const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
pub const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
pub const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
pub fn iri(namespace: &str, name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("{namespace}{name}"))
}

pub fn subject(term: &Term) -> Result<NamedOrBlankNode> {
    match term {
        Term::NamedNode(n) => Ok(n.clone().into()),
        Term::BlankNode(n) => Ok(n.clone().into()),
        _ => Err(Error::Invalid(format!(
            "Expected a shape IRI or blank node, got {term}"
        ))),
    }
}

pub fn with_union(data: &Dataset) -> Dataset {
    let mut union = data.clone();
    union.extend(
        data.iter()
            .map(|q| Quad::new(q.subject, q.predicate, q.object, GraphName::DefaultGraph)),
    );
    union
}

pub fn commit_additions(data: &mut Dataset, before: &Dataset, after: &Dataset) {
    data.extend(
        after
            .iter()
            .filter(|q| !before.contains(*q))
            .map(|q| q.into_owned()),
    );
}

pub fn objects(store: &Dataset, node: &Term, predicate: &NamedNode) -> Result<Vec<Term>> {
    let Ok(node) = subject(node) else {
        return Ok(Vec::new());
    };
    Ok(store
        .quads_for_pattern(
            Some(node.as_ref()),
            Some(predicate.as_ref()),
            None,
            Some(GraphNameRef::DefaultGraph),
        )
        .map(|q| q.object.into_owned())
        .collect())
}

pub fn subjects(
    store: &Dataset,
    predicate: &NamedNode,
    object: Option<&Term>,
) -> Result<Vec<Term>> {
    Ok(store
        .quads_for_pattern(
            None,
            Some(predicate.as_ref()),
            object.map(Term::as_ref),
            Some(GraphNameRef::DefaultGraph),
        )
        .map(|q| q.subject.into_owned().into())
        .collect())
}
