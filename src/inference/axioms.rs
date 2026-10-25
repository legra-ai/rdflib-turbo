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

fn triple(s: impl Into<Term>, p: NamedNode, o: impl Into<Term>) -> GeneralizedQuad {
    GeneralizedQuad::new(s, p, o, GraphName::DefaultGraph)
}

enum DatatypeTyping<'a> {
    Implicit {
        literal: &'a Term,
        alias: Option<&'a Term>,
        datatype: NamedNode,
    },
    Explicit {
        subject: &'a Term,
        datatype: NamedNode,
    },
}

impl<'a> DatatypeTyping<'a> {
    fn from_quad(quad: &'a GeneralizedQuad, allowed: &HashSet<NamedNode>) -> Option<Self> {
        match &quad.object {
            Term::Literal(literal)
                if literal.has_explicit_datatype()
                    && allowed.contains(&literal.datatype().into_owned()) =>
            {
                Some(Self::Implicit {
                    literal: &quad.object,
                    alias: (quad.predicate == Term::from(iri(OWL, "sameAs")))
                        .then_some(&quad.subject),
                    datatype: literal.datatype().into_owned(),
                })
            }
            Term::NamedNode(datatype)
                if quad.predicate == Term::from(iri(RDF, "type")) && allowed.contains(datatype) =>
            {
                Some(Self::Explicit {
                    subject: &quad.subject,
                    datatype: datatype.clone(),
                })
            }
            _ => None,
        }
    }
    fn datatype(&self) -> &NamedNode {
        match self {
            Self::Implicit { datatype, .. } | Self::Explicit { datatype, .. } => datatype,
        }
    }
    fn subjects(&self) -> impl Iterator<Item = &Term> {
        // bounded: the implicit literal and at most one source sameAs alias.
        let (first, second) = match self {
            Self::Implicit { literal, alias, .. } => (Some(*literal), *alias),
            Self::Explicit { subject, .. } => (Some(*subject), None),
        };
        first.into_iter().chain(second)
    }
    fn initial(&self, data: &impl Graph, additions: &mut Pending) -> Result<()> {
        if let Self::Implicit {
            literal,
            alias,
            datatype,
        } = self
        {
            additions.push(
                data,
                triple((*literal).clone(), iri(RDF, "type"), datatype.clone()),
            )?;
            if let Some(alias) = alias {
                // Original OWL7.6.2's explicit-map reverse edge classifies the
                // alias as rdfs:Datatype; its base literal type is not copied.
                additions.push(
                    data,
                    triple((*alias).clone(), iri(RDF, "type"), iri(RDFS, "Datatype")),
                )?;
            }
        }
        Ok(())
    }
}

pub(super) fn owl(
    data: &mut impl Graph,
    combined: bool,
    diagnostics: &mut Diagnostics,
    context: &LexicalContext,
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
        let Some(typing) = DatatypeTyping::from_quad(&q, &datatypes) else {
            continue;
        };
        used.insert(typing.datatype().clone());
        typing.initial(data, &mut additions)?;
        let mut parent = typing
            .datatype()
            .as_str()
            .strip_prefix(XSD)
            .and_then(datatype_parent);
        while let Some(name) = parent {
            let datatype = iri(XSD, name);
            used.insert(datatype.clone());
            for subject in typing.subjects() {
                additions.push(
                    data,
                    triple(subject.clone(), iri(RDF, "type"), datatype.clone()),
                )?;
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
    for quad in additions {
        data.insert(quad)?;
    }
    Ok(())
}

pub(super) fn resources(data: &impl Graph) -> impl Iterator<Item = Result<GeneralizedQuad>> + '_ {
    data.quads(None, None, None)
        .flat_map(|q| {
            let q = match q {
                Ok(q) => q,
                Err(error) => return [Some(Err(error)), None],
            };
            [
                Some(Ok(triple(
                    q.subject,
                    iri(RDF, "type"),
                    iri(RDFS, "Resource"),
                ))),
                Some(Ok(triple(
                    q.object,
                    iri(RDF, "type"),
                    iri(RDFS, "Resource"),
                ))),
            ]
        })
        .flatten()
}

#[cfg(test)]
#[path = "axioms_tests.rs"]
mod tests;
