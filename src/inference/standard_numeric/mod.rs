//! Restricted standard numeric clauses executed on typed streaming relations.
mod bindings;
mod source;

use super::backend::Graph;
use super::diagnostics::Diagnostics;
use super::{InferenceOutcome, InferenceRequest, Pending};
use crate::{Error, Result};
use bindings::{Bindings, Cell};
use datalog_structural_model::{Atom, BuiltinFilter, Literal as Body, Rule};
use oxigraph::model::*;
use rdf_reasoning::{
    OWL_NUMERIC_TYPE_VIOLATION_RELATION, OwlNumericMembershipRuleId, OwlNumericRuleId,
    OwlNumericTypeViolation, TRIPLE_RELATION,
};
use source::{DATATYPES, Source};

pub(super) const MODE: &str = "owl2-rl-numeric-core";
struct Program {
    rules: Vec<Rule>,
}
impl Program {
    fn new() -> Self {
        // bounded: exactly four definitions from the published numeric catalogue.
        Self {
            rules: OwlNumericRuleId::ALL
                .iter()
                .map(|id| id.definition())
                .chain(
                    OwlNumericMembershipRuleId::ALL
                        .iter()
                        .map(|id| id.definition()),
                )
                .collect(),
        }
    }

    fn walk<G: Graph>(
        &self,
        source: &Source<'_, G>,
        rule: &Rule,
        index: usize,
        binding: &Bindings,
        emit: &mut impl FnMut(&Atom, &Bindings) -> Result<()>,
    ) -> Result<()> {
        let Some(body) = rule.body.get(index) else {
            return emit(&rule.head, binding);
        };
        match body {
            Body::Positive(atom) => source.facts(atom, binding, |row| {
                if let Some(next) = binding.join(atom, row)? {
                    self.walk(source, rule, index + 1, &next, emit)?;
                }
                Ok(())
            }),
            Body::Filter(BuiltinFilter::Ne(left, right)) => {
                if binding.resolve(left)? != binding.resolve(right)? {
                    self.walk(source, rule, index + 1, binding, emit)?;
                }
                Ok(())
            }
            _ => Err(Error::Unsupported(
                "Unsupported operator in numeric clause".into(),
            )),
        }
    }

    fn apply<G: Graph>(
        &self,
        graph: &G,
        pending: &mut Pending,
        diagnostics: &mut Diagnostics,
    ) -> Result<()> {
        let source = Source { graph };
        // Validate the complete selected numeric domain even on an empty join result.
        source.literals(None, |_, _| Ok(()))?;
        for rule in &self.rules {
            if !rule.is_safe() {
                return Err(Error::Unsupported("Unsafe shared numeric clause".into()));
            }
            self.walk(
                &source,
                rule,
                0,
                &Bindings::default(),
                &mut |head, binding| {
                    match head.predicate.as_str() {
                        TRIPLE_RELATION if head.terms.len() == 3 => {
                            pending.push(
                                graph,
                                GeneralizedQuad::new(
                                    binding.rdf(&head.terms[0])?,
                                    binding.rdf(&head.terms[1])?,
                                    binding.rdf(&head.terms[2])?,
                                    GraphName::DefaultGraph,
                                ),
                            )?;
                        }
                        OWL_NUMERIC_TYPE_VIOLATION_RELATION if head.terms.len() == 2 => {
                            let literal = binding.rdf(&head.terms[0])?;
                            let datatype = binding.rdf(&head.terms[1])?;
                            let Term::NamedNode(datatype) = datatype else {
                                return Err(Error::Unsupported(
                                    "Invalid numeric diagnostic datatype".into(),
                                ));
                            };
                            let datatype = DATATYPES
                                .iter()
                                .copied()
                                .find(|candidate| candidate.iri() == datatype.as_str())
                                .ok_or_else(|| {
                                    Error::Unsupported(
                                        "Unsupported numeric diagnostic datatype".into(),
                                    )
                                })?;
                            let Cell::Rdf(Term::Literal(parsed)) = Cell::Rdf(literal.clone())
                            else {
                                return Err(Error::Unsupported(
                                    "Numeric diagnostic requires literal".into(),
                                ));
                            };
                            let value = Source::<G>::parse(&parsed)?;
                            let value = value.ok_or_else(|| {
                                Error::Unsupported(
                                    "Numeric diagnostic requires interpreted value".into(),
                                )
                            })?;
                            let violation =
                                OwlNumericTypeViolation::check_assertion(literal, value, datatype)
                                    .expect_err(
                                        "shared nonmembership must contradict the asserted type",
                                    );
                            diagnostics.add(format!(
                                "numeric/dt-not-type: {} is not in <{}>",
                                violation.literal(),
                                violation.datatype().iri()
                            ));
                        }
                        _ => {
                            return Err(Error::Unsupported(
                                "Unsupported numeric clause head".into(),
                            ));
                        }
                    }
                    Ok(())
                },
            )?;
        }
        Ok(())
    }
}

pub(super) fn expand(
    graph: &mut impl Graph,
    request: InferenceRequest,
) -> Result<InferenceOutcome> {
    let mut pending = Pending::default();
    let mut diagnostics = Diagnostics::new(request.prior_error_messages, 0);
    Program::new().apply(graph, &mut pending, &mut diagnostics)?;
    // None of these four clauses introduces a literal or an incompatible type.
    // One immutable pass reaches the same fixed point without scanning new output.
    for head in pending.into_heads() {
        graph.insert_head(head)?;
    }
    diagnostics.finish(graph, false, |_| Ok(false))
}
