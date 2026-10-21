//! Optional finite reference axioms, initialized before the first inference round.

use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::*;

use super::backend::Graph;
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
                for quad in data.quads(None, Some(predicate.as_ref()), None) {
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

    pub(super) fn insert(&self, mut insert: impl FnMut(Quad)) -> Result<()> {
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
                        })?);
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
                ));
            }
        }
        Ok(())
    }
}
