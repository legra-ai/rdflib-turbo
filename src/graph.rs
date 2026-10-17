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
