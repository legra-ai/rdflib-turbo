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
    for quad in data.quads(None, Some(iri(OWL, "differentFrom").as_ref().into()), None) {
        let quad = quad?;
        let left: Term = quad.subject;
        let right = quad.object;
        for (left, right) in [(&left, &right), (&right, &left)] {
            if has(data, left, same.as_ref().into(), right)? {
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
    for declaration in data.quads(None, Some(iri(OWL, "disjointWith").as_ref().into()), None) {
        let declaration = declaration?;
        let left: Term = declaration.subject;
        let right = declaration.object;
        for member in data.quads(None, Some(rdf_type.as_ref().into()), Some(left.as_ref())) {
            let member: Term = member?.subject;
            if has(data, &member, rdf_type.as_ref().into(), &right)? {
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
        for declaration in data.quads(
            None,
            Some(iri(RDF, "type").as_ref().into()),
            Some(class.as_ref()),
        ) {
            let predicate = declaration?.subject;
            for quad in data.quads(None, Some(predicate.as_ref()), None) {
                let quad = quad?;
                let left: Term = quad.subject;
                if asymmetric {
                    if has(data, &quad.object, predicate.as_ref(), &left)? {
                        errors.add(format!(
                            "Erroneous usage of asymmetric property {} on {} and {}",
                            text(&predicate),
                            text(&left),
                            text(&quad.object)
                        ));
                    }
                } else if left == quad.object {
                    errors.add(format!(
                        "Irreflexive property used on {} with {}",
                        text(&left),
                        text(&predicate)
                    ));
                }
            }
        }
    }
    Ok(())
}

fn disjoint_properties(data: &impl Graph, errors: &mut Diagnostics) -> Result<()> {
    for declaration in data.quads(
        None,
        Some(iri(OWL, "propertyDisjointWith").as_ref().into()),
        None,
    ) {
        let declaration = declaration?;
        let left = declaration.subject;
        let right = declaration.object;
        for quad in data.quads(None, Some(left.as_ref()), None) {
            let quad = quad?;
            let member: Term = quad.subject;
            if has(data, &member, right.as_ref(), &quad.object)? {
                errors.add(format!(
                    "Erroneous usage of disjoint properties {} and {} on {} and {}",
                    text(&left),
                    text(&right),
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
    for quad in data.quads(None, Some(rdf_type.as_ref().into()), Some(nothing.as_ref())) {
        errors.add(format!(
            "{} is defined of type 'Nothing'",
            text(&quad?.subject)
        ));
    }
    for declaration in data.quads(None, Some(iri(OWL, "complementOf").as_ref().into()), None) {
        let declaration = declaration?;
        let left: Term = declaration.subject;
        let right = declaration.object;
        for member in data.quads(None, Some(rdf_type.as_ref().into()), Some(left.as_ref())) {
            if has(data, &member?.subject, rdf_type.as_ref().into(), &right)? {
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
