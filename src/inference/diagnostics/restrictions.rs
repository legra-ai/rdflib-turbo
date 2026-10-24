//! Property-assertion conflicts and the default-value cardinality-zero rule.

use oxigraph::model::*;

use super::{Diagnostics, has, objects, text};
use crate::Result;
use crate::graph::{RDF, iri};
use crate::inference::OWL;
use crate::inference::backend::Graph;

pub(super) fn collect(
    data: &impl Graph,
    errors: &mut Diagnostics,
    is_zero: impl Fn(&Term) -> Result<bool>,
) -> Result<()> {
    negative_assertions(data, errors)?;
    for qualified in [false, true] {
        let property = iri(
            OWL,
            if qualified {
                "maxQualifiedCardinality"
            } else {
                "maxCardinality"
            },
        );
        for declaration in data.quads(None, Some(property.as_ref().into()), None) {
            let declaration = declaration?;
            if !is_zero(&declaration.object)? {
                continue;
            }
            let restriction: Term = declaration.subject;
            for predicate in objects(data, &restriction, &iri(OWL, "onProperty")) {
                let predicate = predicate?;
                for quad in data.quads(None, Some(predicate.as_ref()), None) {
                    let quad = quad?;
                    let subject: Term = quad.subject;
                    if !has(
                        data,
                        &subject,
                        iri(RDF, "type").as_ref().into(),
                        &restriction,
                    )? {
                        continue;
                    }
                    if qualified {
                        for class in objects(data, &restriction, &iri(OWL, "onClass")) {
                            let class = class?;
                            if class == Term::from(iri(OWL, "Thing"))
                                || has(
                                    data,
                                    &quad.object,
                                    iri(RDF, "type").as_ref().into(),
                                    &class,
                                )?
                            {
                                errors.add(format!("Erroneous usage of maximum qualified cardinality with {}, {} and {}", text(&restriction), text(&class), text(&quad.object)));
                            }
                        }
                    } else {
                        errors.add(format!(
                            "Erroneous usage of maximum cardinality with {} and {}",
                            text(&restriction),
                            text(&quad.object)
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

fn negative_assertions(data: &impl Graph, errors: &mut Diagnostics) -> Result<()> {
    for declaration in data.quads(
        None,
        Some(iri(OWL, "sourceIndividual").as_ref().into()),
        None,
    ) {
        let declaration = declaration?;
        let assertion: Term = declaration.subject;
        let source = declaration.object;
        for predicate in objects(data, &assertion, &iri(OWL, "assertionProperty")) {
            let predicate = predicate?;
            for (property, kind) in [("targetIndividual", "object"), ("targetValue", "datatype")] {
                for target in objects(data, &assertion, &iri(OWL, property)) {
                    let target = target?;
                    if has(data, &source, predicate.as_ref(), &target)? {
                        errors.add(format!(
                            "Negative ({kind}) property assertion violated for: ({}, {}, {})",
                            text(&source),
                            text(&predicate),
                            text(&target)
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}
