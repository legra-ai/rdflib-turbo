//! One-time entailments used by the pinned pySHACL inference modes.
//! Kept separate from the cyclic rules so RDFS resource initialization does
//! not accidentally extend to terms introduced in later rounds.

use std::collections::HashSet;

use oxigraph::model::*;

use super::OWL;
use crate::graph::{RDF, RDFS, XSD, iri};

const XSD_TYPES: &[&str] = &[
    "integer",
    "decimal",
    "nonNegativeInteger",
    "nonPositiveInteger",
    "negativeInteger",
    "positiveInteger",
    "long",
    "int",
    "short",
    "byte",
    "unsignedLong",
    "unsignedInt",
    "unsignedShort",
    "unsignedByte",
    "float",
    "double",
    "string",
    "normalizedString",
    "token",
    "language",
    "Name",
    "NCName",
    "NMTOKEN",
    "boolean",
    "hexBinary",
    "base64Binary",
    "anyURI",
    "dateTimeStamp",
    "dateTime",
    "time",
    "date",
];

fn datatype_parent(name: &str) -> Option<&'static str> {
    Some(match name {
        "dateTimeStamp" => "dateTime",
        "integer" => "decimal",
        "long" | "nonNegativeInteger" | "nonPositiveInteger" => "integer",
        "int" => "long",
        "short" => "int",
        "byte" => "short",
        "positiveInteger" | "unsignedLong" => "nonNegativeInteger",
        "negativeInteger" => "nonPositiveInteger",
        "unsignedInt" => "unsignedLong",
        "unsignedShort" => "unsignedInt",
        "unsignedByte" => "unsignedShort",
        "normalizedString" => "string",
        "token" => "normalizedString",
        "language" | "Name" => "token",
        "NCName" | "NMTOKEN" => "Name",
        _ => return None,
    })
}

fn triple(s: impl Into<NamedOrBlankNode>, p: NamedNode, o: impl Into<Term>) -> Quad {
    Quad::new(s, p, o, GraphName::DefaultGraph)
}

pub(super) fn owl(data: &mut Dataset, combined: bool) {
    let datatypes: HashSet<NamedNode> = XSD_TYPES
        .iter()
        .map(|n| iri(XSD, n))
        .chain([iri(RDFS, "Literal")])
        .chain(["XMLLiteral", "HTML", "langString", "PlainLiteral"].map(|n| iri(RDF, n)))
        .collect();
    let mut used = HashSet::new();
    let mut typed = Vec::new();
    for q in data.iter() {
        if let TermRef::Literal(literal) = q.object
            && (literal.has_explicit_datatype() || literal.language().is_some())
            && datatypes.contains(&literal.datatype().into_owned())
        {
            used.insert(literal.datatype().into_owned());
            // Generalized RDF triples with literal subjects cannot be stored
            // in oxrdf::Dataset. Track datatype use without inventing nodes.
            typed.push((None, literal.datatype().into_owned()));
            if q.predicate == iri(OWL, "sameAs").as_ref() {
                typed.push((
                    Some(q.subject.into_owned()),
                    literal.datatype().into_owned(),
                ));
            }
        }
        if q.predicate == iri(RDF, "type").as_ref()
            && let TermRef::NamedNode(datatype) = q.object
            && datatypes.contains(&datatype.into_owned())
        {
            used.insert(datatype.into_owned());
            typed.push((Some(q.subject.into_owned()), datatype.into_owned()));
        }
    }
    let mut additions = Vec::new();
    for (node, datatype) in typed {
        if let Some(node) = &node {
            additions.push(triple(node.clone(), iri(RDF, "type"), datatype.clone()));
        }
        let mut parent = datatype
            .as_str()
            .strip_prefix(XSD)
            .and_then(datatype_parent);
        while let Some(name) = parent {
            let datatype = iri(XSD, name);
            used.insert(datatype.clone());
            if let Some(node) = &node {
                additions.push(triple(node.clone(), iri(RDF, "type"), datatype));
            }
            parent = datatype_parent(name);
        }
    }
    additions.extend(
        datatypes
            .into_iter()
            .map(|datatype| triple(datatype, iri(RDF, "type"), iri(RDFS, "Datatype"))),
    );
    additions.extend(
        ["Thing", "Nothing"].map(|n| triple(iri(OWL, n), iri(RDF, "type"), iri(OWL, "Class"))),
    );
    for (ns, names) in [
        (RDFS, &["label", "comment", "seeAlso", "isDefinedBy"][..]),
        (
            OWL,
            &[
                "deprecated",
                "versionInfo",
                "priorVersion",
                "backwardCompatibleWith",
                "incompatibleWith",
            ][..],
        ),
    ] {
        additions.extend(
            names
                .iter()
                .map(|n| triple(iri(ns, n), iri(RDF, "type"), iri(OWL, "AnnotationProperty"))),
        );
    }
    // The reference emits one directed statement per disjoint top-level pair,
    // only when both datatypes occur. PlainLiteral overlaps string and XMLLiteral.
    let disjoint: Vec<_> = [
        "anyURI",
        "base64Binary",
        "boolean",
        "dateTime",
        "decimal",
        "double",
        "float",
        "hexBinary",
        "string",
    ]
    .map(|n| iri(XSD, n))
    .into_iter()
    .chain([iri(RDF, "PlainLiteral"), iri(RDF, "XMLLiteral")])
    .collect();
    for (i, left) in disjoint.iter().enumerate() {
        for right in &disjoint[i + 1..] {
            if used.contains(left)
                && used.contains(right)
                && !(left == &iri(XSD, "string") && right == &iri(RDF, "PlainLiteral"))
                && left != &iri(RDF, "PlainLiteral")
            {
                additions.push(triple(
                    left.clone(),
                    iri(OWL, "disjointWith"),
                    right.clone(),
                ));
            }
        }
    }
    if combined {
        for (left, right) in [
            (iri(OWL, "Thing"), iri(RDFS, "Resource")),
            (iri(RDFS, "Class"), iri(OWL, "Class")),
            (iri(OWL, "DataRange"), iri(RDFS, "Datatype")),
        ] {
            additions.push(triple(left, iri(OWL, "equivalentClass"), right));
        }
    }
    data.extend(additions);
}

pub(super) fn resources(data: &Dataset) -> Vec<Quad> {
    data.iter()
        .flat_map(|q| {
            let mut nodes = vec![q.subject.into_owned()];
            match q.object {
                TermRef::NamedNode(n) => nodes.push(n.into_owned().into()),
                TermRef::BlankNode(n) => nodes.push(n.into_owned().into()),
                TermRef::Literal(_) => {}
            }
            nodes
                .into_iter()
                .map(|node| triple(node, iri(RDF, "type"), iri(RDFS, "Resource")))
        })
        .collect()
}
