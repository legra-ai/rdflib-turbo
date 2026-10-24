use oxigraph::model::*;
pub const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
pub const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
pub const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
pub fn iri(namespace: &str, name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("{namespace}{name}"))
}

#[cfg(test)]
pub fn with_union(data: &oxrdf::GeneralizedDataset) -> oxrdf::GeneralizedDataset {
    let mut union = data.clone();
    union.extend(
        data.iter().map(|q| {
            GeneralizedQuad::new(q.subject, q.predicate, q.object, GraphName::DefaultGraph)
        }),
    );
    union
}
