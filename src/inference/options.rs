//! Optional finite reference axioms, initialized before the first inference round.

#[cfg(all(test, feature = "allocation-profiling"))]
use crate::allocation_tests::trace;
use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::*;

use super::backend::Graph;
use super::{InferenceOutcome, InferenceRequest, expand_inner};
use crate::graph::{RDF, RDFS, iri};
use crate::{Error, Result};

#[derive(Clone, Copy, Default)]
pub(crate) struct InferenceOptions {
    pub axiomatic_triples: bool,
    pub datatype_axioms: bool,
}

pub(super) struct PreparedOptions<'a> {
    mode: &'a str,
    options: InferenceOptions,
    container_max: usize,
}

impl<'a> PreparedOptions<'a> {
    pub(super) fn new(data: &impl Graph, mode: &'a str, options: InferenceOptions) -> Result<Self> {
        let mut container_max = 0;
        if options.axiomatic_triples && mode != "owlrl" {
            let mut next = 1_usize;
            loop {
                let predicate = iri(RDF, &format!("_{next}"));
                let mut count = 0;
                for quad in data.quads(None, Some(predicate.as_ref().into()), None) {
                    let quad = quad?;
                    // A union can repeat SPO in several contexts. An exact indexed
                    // probe chooses one stable representative without retaining a set.
                    let representative = data
                        .quads(
                            Some(quad.subject.as_ref()),
                            Some(quad.predicate.as_ref()),
                            Some(quad.object.as_ref()),
                        )
                        .next()
                        .transpose()?;
                    if representative.is_some_and(|first| first.graph_name == quad.graph_name) {
                        count += 1;
                    }
                }
                if count == 0 {
                    break;
                }
                container_max = next
                    .checked_add(count - 1)
                    .ok_or_else(|| Error::Runtime("Container index overflow".into()))?;
                next = container_max
                    .checked_add(1)
                    .ok_or_else(|| Error::Runtime("Container index overflow".into()))?;
            }
        }
        Ok(Self {
            mode,
            options,
            container_max,
        })
    }

    pub(super) fn insert(&self, mut insert: impl FnMut(Quad) -> Result<()>) -> Result<()> {
        for (enabled, rdfs, owl) in [
            (
                self.options.axiomatic_triples,
                include_str!("../../vendor/owlrl-7.6.2-axioms/RDFS_Axiomatic_Triples.nt"),
                include_str!("../../vendor/owlrl-7.6.2-axioms/OWLRL_Axiomatic_Triples.nt"),
            ),
            (
                self.options.datatype_axioms,
                include_str!("../../vendor/owlrl-7.6.2-axioms/RDFS_D_Axiomatic_Triples.nt"),
                include_str!("../../vendor/owlrl-7.6.2-axioms/OWLRL_D_Axiomatic_Triples.nt"),
            ),
        ] {
            if !enabled {
                continue;
            }
            for (selected, table) in [(self.mode != "owlrl", rdfs), (self.mode != "rdfs", owl)] {
                if selected {
                    for quad in
                        RdfParser::from_format(RdfFormat::NTriples).for_slice(table.as_bytes())
                    {
                        insert(quad.map_err(|error| {
                            Error::Runtime(format!("Invalid built-in axioms: {error}"))
                        })?)?;
                    }
                }
            }
        }
        for index in 1..=self.container_max {
            let subject = iri(RDF, &format!("_{index}"));
            for (predicate, object) in [
                (iri(RDF, "type"), iri(RDF, "Property")),
                (iri(RDFS, "domain"), iri(RDFS, "Resource")),
                (iri(RDFS, "range"), iri(RDFS, "Resource")),
                (iri(RDF, "type"), iri(RDFS, "ContainerMembershipProperty")),
            ] {
                insert(Quad::new(
                    subject.clone(),
                    predicate,
                    object,
                    GraphName::DefaultGraph,
                ))?;
            }
        }
        Ok(())
    }
}

pub(super) fn expand_configured<'a>(
    data: &mut impl Graph,
    mode: &'a str,
    request: InferenceRequest,
) -> Result<(PreparedOptions<'a>, InferenceOutcome)> {
    #[cfg(all(test, feature = "allocation-profiling"))]
    trace::mark("configured_enter", 0, (0, 0));
    match mode {
        "none" | "" => {
            return Ok((
                PreparedOptions::new(data, mode, InferenceOptions::default())?,
                InferenceOutcome::default(),
            ));
        }
        "rdfs" | "owlrl" | "both" => {}
        _ => return Err(Error::Unsupported(format!("inference: {mode}"))),
    }
    let prepared = PreparedOptions::new(data, mode, request.options)?;
    if mode != "owlrl" && request.options.datatype_axioms {
        // Original RDFS add_d_axioms directly publishes each input object's
        // explicit datatype before adding the finite schema table. Retain only
        // actual destination-absent output, never a copy of the input relation.
        let mut additions = super::Pending::default();
        for quad in data.quads(None, None, None) {
            let quad = quad?;
            if let Term::Literal(literal) = &quad.object
                && literal.language().is_none()
                && literal.has_explicit_datatype()
            {
                let typing = GeneralizedQuad::new(
                    quad.object.clone(),
                    iri(RDF, "type"),
                    literal.datatype(),
                    GraphName::DefaultGraph,
                );
                if !data.contains_destination(typing.as_ref())? {
                    additions.insert(typing);
                }
            }
        }
        for quad in additions.into_heads() {
            data.insert_head(quad)?;
        }
    }
    prepared.insert(|quad| data.insert(quad.into()))?;
    #[cfg(all(test, feature = "allocation-profiling"))]
    trace::mark("configured_initialization_done", 0, (0, 0));
    let outcome = expand_inner(data, mode, request)?;
    #[cfg(all(test, feature = "allocation-profiling"))]
    trace::mark("configured_return", 0, (0, 0));
    Ok((prepared, outcome))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::XSD;
    use oxrdf::GeneralizedDataset;

    #[test]
    fn rdfs_datatype_axioms_type_input_literals_in_the_requested_destination() {
        for datatype_axioms in [false, true] {
            let value = Literal::new_typed_literal("7", iri(XSD, "integer"));
            let input = GeneralizedQuad::new(
                iri("urn:", "s"),
                iri("urn:", "p"),
                value.clone(),
                GraphName::DefaultGraph,
            );
            let mut data = GeneralizedDataset::from_iter([input.clone()]);
            let request = InferenceRequest {
                options: InferenceOptions {
                    datatype_axioms,
                    axiomatic_triples: false,
                },
                ..InferenceRequest::default()
            };
            super::super::expand(&mut data, "rdfs", request).unwrap();
            for kind in ["integer", "decimal"] {
                assert_eq!(
                    data.contains(&GeneralizedQuad::new(
                        value.clone(),
                        iri(RDF, "type"),
                        iri(XSD, kind),
                        GraphName::DefaultGraph
                    )),
                    datatype_axioms,
                    "{kind}"
                );
            }
            let store = oxigraph::store::Store::new().unwrap();
            store.insert_generalized(&input).unwrap();
            let destination = GraphName::from(iri("urn:", "destination"));
            let request = InferenceRequest {
                options: InferenceOptions {
                    datatype_axioms,
                    axiomatic_triples: false,
                },
                ..InferenceRequest::default()
            };
            let mut transaction = store.start_transaction().unwrap();
            super::super::scoped::expand(
                &mut transaction,
                "rdfs",
                Some(GraphName::DefaultGraph),
                destination.clone(),
                request,
            )
            .unwrap();
            for kind in ["integer", "decimal"] {
                assert_eq!(
                    transaction
                        .contains_generalized(&GeneralizedQuad::new(
                            value.clone(),
                            iri(RDF, "type"),
                            iri(XSD, kind),
                            destination.clone()
                        ))
                        .unwrap(),
                    datatype_axioms && kind == "integer",
                    "destination {kind}"
                );
                assert!(
                    !transaction
                        .contains_generalized(&GeneralizedQuad::new(
                            value.clone(),
                            iri(RDF, "type"),
                            iri(XSD, kind),
                            GraphName::DefaultGraph
                        ))
                        .unwrap()
                );
            }
        }
    }
}
