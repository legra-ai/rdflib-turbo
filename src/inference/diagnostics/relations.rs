//! Positive inconsistency conditions checked through existing SPO indexes.

use super::{Diagnostics, has, text};
use crate::Result;
use crate::graph::{RDF, iri};
use crate::inference::OWL;
use crate::inference::backend::Graph;
use oxigraph::model::*;

pub(super) fn collect(data: &impl Graph, errors: &mut Diagnostics) -> Result<()> {
    equality(data, errors)?;
    disjoint_classes(data, errors)?;
    properties(data, errors)?;
    disjoint_properties(data, errors)?;
    nothing_and_complements(data, errors)
}

fn equality(data: &impl Graph, errors: &mut Diagnostics) -> Result<()> {
    let same = iri(OWL, "sameAs");
    // Start from conflicts, not the potentially quadratic equality closure.
    for quad in data.quads(None, Some(iri(OWL, "differentFrom").as_ref()), None) {
        let quad = quad?;
        let left: Term = quad.subject.into();
        let right = quad.object;
        for (left, right) in [(&left, &right), (&right, &left)] {
            if has(data, left, &same, right)? {
                errors.add(format!(
                    "'sameAs' and 'differentFrom' cannot be used on the same subject-object pair: ({}, {})",
                    text(left), text(right)
                ));
            }
        }
    }
    Ok(())
}

fn disjoint_classes(data: &impl Graph, errors: &mut Diagnostics) -> Result<()> {
    let rdf_type = iri(RDF, "type");
    for declaration in data.quads(None, Some(iri(OWL, "disjointWith").as_ref()), None) {
        let declaration = declaration?;
        let left: Term = declaration.subject.into();
        let right = declaration.object;
        for member in data.quads(None, Some(rdf_type.as_ref()), Some(left.as_ref())) {
            let member: Term = member?.subject.into();
            if has(data, &member, &rdf_type, &right)? {
                errors.add(format!(
                    "Disjoint classes {} and {} have a common individual {}",
                    text(&left),
                    text(&right),
                    text(&member)
                ));
            }
        }
    }
    Ok(())
}

fn properties(data: &impl Graph, errors: &mut Diagnostics) -> Result<()> {
    for (kind, asymmetric) in [("IrreflexiveProperty", false), ("AsymmetricProperty", true)] {
        let class: Term = iri(OWL, kind).into();
        for declaration in data.quads(None, Some(iri(RDF, "type").as_ref()), Some(class.as_ref())) {
            let NamedOrBlankNode::NamedNode(predicate) = declaration?.subject else {
                continue;
            };
            for quad in data.quads(None, Some(predicate.as_ref()), None) {
                let quad = quad?;
                let left: Term = quad.subject.into();
                if asymmetric {
                    if has(data, &quad.object, &predicate, &left)? {
                        errors.add(format!(
                            "Erroneous usage of asymmetric property {} on {} and {}",
                            predicate.as_str(),
                            text(&left),
                            text(&quad.object)
                        ));
                    }
                } else if left == quad.object {
                    errors.add(format!(
                        "Irreflexive property used on {} with {}",
                        text(&left),
                        predicate.as_str()
                    ));
                }
            }
        }
    }
    Ok(())
}

fn disjoint_properties(data: &impl Graph, errors: &mut Diagnostics) -> Result<()> {
    for declaration in data.quads(None, Some(iri(OWL, "propertyDisjointWith").as_ref()), None) {
        let declaration = declaration?;
        let NamedOrBlankNode::NamedNode(left) = declaration.subject else {
            continue;
        };
        let Term::NamedNode(right) = declaration.object else {
            continue;
        };
        for quad in data.quads(None, Some(left.as_ref()), None) {
            let quad = quad?;
            let member: Term = quad.subject.into();
            if has(data, &member, &right, &quad.object)? {
                errors.add(format!(
                    "Erroneous usage of disjoint properties {} and {} on {} and {}",
                    left.as_str(),
                    right.as_str(),
                    text(&member),
                    text(&quad.object)
                ));
            }
        }
    }
    Ok(())
}

fn nothing_and_complements(data: &impl Graph, errors: &mut Diagnostics) -> Result<()> {
    let rdf_type = iri(RDF, "type");
    let nothing: Term = iri(OWL, "Nothing").into();
    for quad in data.quads(None, Some(rdf_type.as_ref()), Some(nothing.as_ref())) {
        errors.add(format!(
            "{} is defined of type 'Nothing'",
            text(&quad?.subject.into())
        ));
    }
    for declaration in data.quads(None, Some(iri(OWL, "complementOf").as_ref()), None) {
        let declaration = declaration?;
        let left: Term = declaration.subject.into();
        let right = declaration.object;
        for member in data.quads(None, Some(rdf_type.as_ref()), Some(left.as_ref())) {
            if has(data, &member?.subject.into(), &rdf_type, &right)? {
                // The pinned reference prints the complement class in its
                // element field, rather than the individual being checked.
                errors.add(format!(
                    "Violation of complementarity for classes {} and {} on element {}",
                    text(&left),
                    text(&right),
                    text(&right)
                ));
            }
        }
    }
    Ok(())
}
