//! The RDFS and OWL 2 RL fixed-point kernel.

#[cfg(all(test, feature = "allocation-profiling"))]
use crate::allocation_tests::trace;
use oxigraph::model::*;
use oxigraph::sparql::SparqlEvaluator;

use super::backend::Graph;
use super::pending::Pending;
use super::rules::{OWL_RULES, PREFIXES, apply_rules};
use super::{
    InferenceOutcome, InferenceRequest, axioms, diagnostics, intersections, literal_values,
    same_as, scheduling, traversal,
};
use crate::{Error, Result};

pub(super) fn expand_inner(
    data: &mut impl Graph,
    mode: &str,
    request: InferenceRequest,
) -> Result<InferenceOutcome> {
    #[cfg(all(test, feature = "allocation-profiling"))]
    trace::mark("kernel_enter", 0, (0, 0));
    match mode {
        "none" | "" => return Ok(InferenceOutcome::default()),
        "rdfs" | "owlrl" | "both" => {}
        _ => return Err(Error::Unsupported(format!("inference: {mode}"))),
    }
    if mode == "rdfs" {
        // RDFS_Semantics.one_time_rules publishes every literal's identity pair
        // directly to its destination, even when the input contains that SPO.
        // Retain only the output delta; no source snapshot is needed.
        #[cfg(all(test, feature = "allocation-profiling"))]
        trace::mark("literal_publications_before", 0, (0, 0));
        let mut literal_publications = Pending::default();
        for quad in data.quads(None, None, None) {
            let mut quad = quad?;
            if quad.object.is_literal() && !data.contains_destination(quad.as_ref())? {
                quad.graph_name = GraphName::DefaultGraph;
                literal_publications.insert(quad);
            }
        }
        #[cfg(all(test, feature = "allocation-profiling"))]
        trace::mark(
            "literal_publications_collected",
            0,
            literal_publications.trace_counts(),
        );
        for quad in literal_publications.into_heads() {
            data.insert_head(quad)?;
        }
        #[cfg(all(test, feature = "allocation-profiling"))]
        trace::mark("literal_publications_done", 0, (0, 0));
        literal_values::substitute(data, &request.lexical_context)?;
        #[cfg(all(test, feature = "allocation-profiling"))]
        trace::mark("literal_values_done", 0, (0, 0));
    }
    let mut diagnostics = diagnostics::Diagnostics::new(
        request.prior_error_messages,
        request.lexical_context.integer_digit_limit,
    );
    // bounded: the fixed RDFS/OWL rule tables above.
    let mut rules = Vec::new();
    if mode != "rdfs" {
        rules.extend_from_slice(OWL_RULES);
        axioms::owl(
            data,
            mode == "both",
            &mut diagnostics,
            &request.lexical_context,
        )?;
    }
    #[cfg(all(test, feature = "allocation-profiling"))]
    trace::mark("resources_before", 0, (0, 0));
    let mut initial_resources = Pending::default();
    if mode != "owlrl" {
        for quad in axioms::resources(data) {
            initial_resources.push(data, quad?)?;
        }
    }
    #[cfg(all(test, feature = "allocation-profiling"))]
    trace::mark("resources_collected", 0, initial_resources.trace_counts());
    // bounded: one prepared query per built-in rule.
    let mut schedule = scheduling::Schedule::for_mode(mode)?;
    let queries = rules
        .iter()
        .map(|(template, pattern)| {
            SparqlEvaluator::new()
                .parse_query(&format!(
                    "{PREFIXES}\nCONSTRUCT {{ {template} }} WHERE {{ {pattern} }}"
                ))
                .map_err(|e| Error::Runtime(format!("Invalid inference rule: {e}")))
        })
        .collect::<Result<Vec<_>>>()?;
    #[cfg(all(test, feature = "allocation-profiling"))]
    trace::mark("rules_prepared", 0, initial_resources.trace_counts());
    #[cfg(all(test, feature = "allocation-profiling"))]
    let mut trace_round = 0;
    loop {
        #[cfg(all(test, feature = "allocation-profiling"))]
        {
            trace_round += 1;
            trace::mark("round_enter", trace_round, initial_resources.trace_counts());
        }
        if mode != "rdfs" {
            diagnostics.round(data)?;
        }
        let mut additions = std::mem::take(&mut initial_resources);
        if mode != "rdfs" {
            same_as::apply(data, &mut additions)?;
        }
        #[cfg(all(test, feature = "allocation-profiling"))]
        trace::mark("schedule_before", trace_round, additions.trace_counts());
        #[cfg(not(all(test, feature = "allocation-profiling")))]
        schedule.apply(data, &mut additions)?;
        #[cfg(all(test, feature = "allocation-profiling"))]
        schedule.apply_traced(data, &mut additions, trace_round)?;
        #[cfg(all(test, feature = "allocation-profiling"))]
        trace::mark("schedule_after", trace_round, additions.trace_counts());
        apply_rules(data, &queries, &mut additions, false)?;
        #[cfg(all(test, feature = "allocation-profiling"))]
        trace::mark("other_queries_after", trace_round, additions.trace_counts());
        if mode != "rdfs" {
            // Lists can themselves be inferred, so discover them each round.
            intersections::additions(data, &mut additions)?;
            traversal::chains(data, &mut additions, diagnostics.witnesses())?;
            traversal::keys(data, &mut additions, diagnostics.witnesses())?;
        }
        if additions.is_empty() {
            #[cfg(all(test, feature = "allocation-profiling"))]
            trace::mark("diagnostics_before", trace_round, additions.trace_counts());
            let outcome = diagnostics.finish(data, mode != "rdfs", |term| {
                diagnostics::cardinality_zero(term, &request.lexical_context)
            })?;
            if mode != "rdfs" {
                data.cleanup_blank_predicates()?;
            }
            #[cfg(all(test, feature = "allocation-profiling"))]
            trace::mark("kernel_return", trace_round, additions.trace_counts());
            return Ok(outcome);
        }
        #[cfg(all(test, feature = "allocation-profiling"))]
        trace::mark("publish_before", trace_round, additions.trace_counts());
        let mut dirty = 0;
        for quad in additions.into_heads() {
            // Conservatively count destination-only publications too. Native
            // membership kernels still run every round and retain their wider view.
            dirty |= schedule.changed(&quad.as_ref().predicate.into_owned());
            data.insert_head(quad)?;
        }
        #[cfg(all(test, feature = "allocation-profiling"))]
        trace::mark("publish_after", trace_round, (0, 0));
        schedule.advance(dirty);
    }
}
