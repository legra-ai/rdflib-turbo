//! One-time entailments used by the pinned pySHACL inference modes.
//! Kept separate from the cyclic rules so RDFS resource initialization does
//! not accidentally extend to terms introduced in later rounds.

use std::collections::HashSet;

use oxigraph::model::*;

use super::backend::Graph;
use super::diagnostic_datatypes::LexicalContext;
use super::diagnostics::Diagnostics;
use super::{OWL, Pending};
use crate::Result;
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

pub(super) fn owl(
    data: &mut impl Graph,
    combined: bool,
    diagnostics: &mut Diagnostics,
    context: &LexicalContext,
    generalized: &mut super::generalized::Generalized,
) -> Result<()> {
    // bounded: the fixed XSD/RDF datatype vocabulary below.
    let datatypes: HashSet<NamedNode> = XSD_TYPES
        .iter()
        .map(|n| iri(XSD, n))
        .chain([iri(RDFS, "Literal")])
        .chain(["XMLLiteral", "HTML", "langString", "PlainLiteral"].map(|n| iri(RDF, n)))
        .collect();
    // bounded: a subset of the fixed datatype vocabulary.
    let mut used = HashSet::new();
    let mut additions = Pending::default();
    for q in data.quads(None, None, None) {
        let q = q?;
        if let Term::Literal(literal) = &q.object
            && literal.language().is_none()
            && literal.has_explicit_datatype()
            && datatypes.contains(&literal.datatype().into_owned())
        {
            diagnostics.literal(literal, |literal| {
                super::diagnostics::valid_lexical(literal, context)
            })?;
        }
        let typed = match &q.object {
            Term::Literal(literal)
                if (literal.has_explicit_datatype() || literal.language().is_some())
                    && datatypes.contains(&literal.datatype().into_owned()) =>
            {
                Some((
                    q.predicate == iri(OWL, "sameAs"),
                    literal.datatype().into_owned(),
                ))
            }
            Term::NamedNode(datatype)
                if q.predicate == iri(RDF, "type") && datatypes.contains(datatype) =>
            {
                Some((true, datatype.clone()))
            }
            _ => None,
        };
        let Some((has_subject, datatype)) = typed else {
            continue;
        };
        used.insert(datatype.clone());
        if let Term::Literal(literal) = &q.object {
            generalized.literal_type(literal, datatype.clone());
        }
        if has_subject {
            additions.push(
                data,
                triple(q.subject.clone(), iri(RDF, "type"), datatype.clone()),
            )?;
        }
        let mut parent = datatype
            .as_str()
            .strip_prefix(XSD)
            .and_then(datatype_parent);
        while let Some(name) = parent {
            let datatype = iri(XSD, name);
            used.insert(datatype.clone());
            if let Term::Literal(literal) = &q.object {
                generalized.literal_type(literal, datatype.clone());
            }
            if has_subject {
                additions.push(data, triple(q.subject.clone(), iri(RDF, "type"), datatype))?;
            }
            parent = datatype_parent(name);
        }
    }
    // bounded: only the fixed datatype and schema axioms below.
    let mut fixed = Vec::new();
    fixed.extend(
        datatypes
            .into_iter()
            .map(|datatype| triple(datatype, iri(RDF, "type"), iri(RDFS, "Datatype"))),
    );
    fixed.extend(
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
        fixed.extend(
            names
                .iter()
                .map(|n| triple(iri(ns, n), iri(RDF, "type"), iri(OWL, "AnnotationProperty"))),
        );
    }
    // The reference emits one directed statement per disjoint top-level pair,
    // only when both datatypes occur. PlainLiteral overlaps string and XMLLiteral.
    // bounded: eleven explicitly enumerated datatypes.
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
                fixed.push(triple(
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
            fixed.push(triple(left, iri(OWL, "equivalentClass"), right));
        }
    }
    for quad in fixed {
        additions.push(data, quad)?;
    }
    for quad in additions.0 {
        data.insert(quad)?;
    }
    Ok(())
}

pub(super) fn resources(data: &impl Graph) -> impl Iterator<Item = Result<Quad>> + '_ {
    data.quads(None, None, None)
        .flat_map(|q| {
            let q = match q {
                Ok(q) => q,
                Err(error) => return [Some(Err(error)), None],
            };
            let object: Option<NamedOrBlankNode> = match q.object {
                Term::NamedNode(n) => Some(n.into()),
                Term::BlankNode(n) => Some(n.into()),
                Term::Literal(_) => None,
            };
            [
                Some(Ok(triple(
                    q.subject,
                    iri(RDF, "type"),
                    iri(RDFS, "Resource"),
                ))),
                object.map(|node| Ok(triple(node, iri(RDF, "type"), iri(RDFS, "Resource")))),
            ]
        })
        .flatten()
}

#[cfg(test)]
#[path = "axioms_tests.rs"]
mod tests;
