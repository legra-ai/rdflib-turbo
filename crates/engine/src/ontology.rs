//! pySHACL ontology inoculation: schema axioms and explicit NamedIndividuals.

use std::collections::{HashMap, HashSet, VecDeque};

use oxigraph::model::*;

use crate::graph::{RDF, RDFS, iri, with_union};

#[cfg(test)]
#[path = "ontology_tests.rs"]
mod tests;

const OWL: &str = "http://www.w3.org/2002/07/owl#";
const RDFS_CLASSES: &[&str] = &[
    "Class",
    "Container",
    "ContainerMembershipProperty",
    "Datatype",
    "Literal",
    "Resource",
];
const RDFS_PROPERTIES: &[&str] = &[
    "comment",
    "domain",
    "isDefinedBy",
    "label",
    "member",
    "range",
    "seeAlso",
    "subClassOf",
    "subPropertyOf",
];
const OWL_CLASSES: &[&str] = &[
    "AllDifferent",
    "AllDisjointClasses",
    "AllDisjointProperties",
    "Annotation",
    "AnnotationProperty",
    "AsymmetricProperty",
    "Axiom",
    "Class",
    "DataRange",
    "DatatypeProperty",
    "DeprecatedClass",
    "DeprecatedProperty",
    "FunctionalProperty",
    "InverseFunctionalProperty",
    "IrreflexiveProperty",
    "NamedIndividual",
    "NegativePropertyAssertion",
    "ObjectProperty",
    "Ontology",
    "OntologyProperty",
    "ReflexiveProperty",
    "Restriction",
    "SymmetricProperty",
    "TransitiveProperty",
];
const OWL_PROPERTIES: &[&str] = &[
    "allValuesFrom",
    "annotatedProperty",
    "annotatedSource",
    "annotatedTarget",
    "assertionProperty",
    "cardinality",
    "complementOf",
    "datatypeComplementOf",
    "differentFrom",
    "disjointUnionOf",
    "disjointWith",
    "distinctMembers",
    "equivalentClass",
    "equivalentProperty",
    "hasKey",
    "hasSelf",
    "hasValue",
    "intersectionOf",
    "inverseOf",
    "maxCardinality",
    "maxQualifiedCardinality",
    "members",
    "minCardinality",
    "minQualifiedCardinality",
    "onClass",
    "onDataRange",
    "onDatatype",
    "onProperties",
    "onProperty",
    "oneOf",
    "propertyChainAxiom",
    "propertyDisjointWith",
    "qualifiedCardinality",
    "sameAs",
    "someValuesFrom",
    "sourceIndividual",
    "targetIndividual",
    "targetValue",
    "unionOf",
    "withRestrictions",
    "backwardCompatibleWith",
    "deprecated",
    "incompatibleWith",
    "priorVersion",
    "versionInfo",
    "bottomDataProperty",
    "topDataProperty",
    "bottomObjectProperty",
    "topObjectProperty",
    "imports",
    "versionIRI",
];

fn recognized(node: NamedNodeRef<'_>, rdfs: &[&str], owl: &[&str]) -> bool {
    node.as_str()
        .strip_prefix(RDFS)
        .is_some_and(|s| rdfs.contains(&s))
        || node
            .as_str()
            .strip_prefix(OWL)
            .is_some_and(|s| owl.contains(&s))
}

#[derive(Default)]
struct Copies {
    blanks: HashMap<BlankNode, BlankNode>,
    pending: VecDeque<(NamedOrBlankNode, bool)>,
}

impl Copies {
    fn subject(&mut self, node: NamedOrBlankNodeRef<'_>) -> NamedOrBlankNode {
        match node {
            NamedOrBlankNodeRef::NamedNode(n) => n.into_owned().into(),
            NamedOrBlankNodeRef::BlankNode(n) => {
                self.pending.push_back((n.into_owned().into(), false));
                self.blanks
                    .entry(n.into_owned())
                    .or_default()
                    .clone()
                    .into()
            }
        }
    }

    fn object(&mut self, node: TermRef<'_>, deep: bool) -> Term {
        match node {
            TermRef::BlankNode(n) => Term::from(self.subject(n.into())),
            TermRef::NamedNode(n) if deep => {
                self.pending.push_back((n.into_owned().into(), true));
                n.into_owned().into()
            }
            _ => node.into_owned(),
        }
    }
}

pub fn inoculate(data: &mut Dataset, ontology: &Dataset) {
    let ontology = with_union(ontology);
    let mut copies = Copies::default();
    let individuals: HashSet<_> = ontology
        .quads_for_pattern(
            None,
            Some(iri(RDF, "type").as_ref()),
            Some(iri(OWL, "NamedIndividual").as_ref().into()),
            Some(GraphNameRef::DefaultGraph),
        )
        .map(|q| q.subject.into_owned())
        .collect();
    for quad in ontology.iter().filter(|q| q.graph_name.is_default_graph()) {
        let schema_type = quad.predicate == iri(RDF, "type").as_ref()
            && matches!(quad.object, TermRef::NamedNode(n) if recognized(n, RDFS_CLASSES, OWL_CLASSES));
        let incoming = match quad.object {
            TermRef::NamedNode(n) => individuals.contains(&n.into_owned().into()),
            TermRef::BlankNode(n) => individuals.contains(&n.into_owned().into()),
            _ => false,
        };
        if schema_type || recognized(quad.predicate, RDFS_PROPERTIES, OWL_PROPERTIES) || incoming {
            let subject = copies.subject(quad.subject);
            let object = copies.object(quad.object, false);
            data.insert(&Quad::new(
                subject,
                quad.predicate,
                object,
                GraphName::DefaultGraph,
            ));
        }
    }
    copies
        .pending
        .extend(individuals.into_iter().map(|n| (n, true)));
    let mut visited = HashSet::new();
    while let Some((node, deep)) = copies.pending.pop_front() {
        if !visited.insert((node.clone(), deep)) {
            continue;
        }
        let subject = copies.subject(node.as_ref());
        for quad in ontology.quads_for_pattern(
            Some(node.as_ref()),
            None,
            None,
            Some(GraphNameRef::DefaultGraph),
        ) {
            let object = copies.object(quad.object, deep);
            data.insert(&Quad::new(
                subject.clone(),
                quad.predicate,
                object,
                GraphName::DefaultGraph,
            ));
        }
    }
}
