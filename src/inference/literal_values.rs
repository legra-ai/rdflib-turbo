//! RDFS one-time substitution joins caller-profile value classes.
use oxigraph::model::{GeneralizedQuad, GraphName, Literal, NamedNode, Term};
use oxigraph::sparql::{PreparedSparqlQuery, SparqlEvaluator};
use rdf_reasoning::{LITERAL_VALUE_RELATION, LiteralRuleId};

use super::Pending;
use super::backend::Graph;
use super::diagnostic_datatypes::{LexicalContext, PythonNumericValue, parse_numeric};
use super::shared::CompiledRule;
#[cfg(all(test, feature = "allocation-profiling"))]
use crate::allocation_tests::trace;
use crate::{Error, Result};

fn value_key(literal: &Literal, context: &LexicalContext) -> Option<Term> {
    if literal.language().is_some()
        || matches!(
            literal.datatype().as_str(),
            "http://www.w3.org/2001/XMLSchema#string"
                | "http://www.w3.org/2001/XMLSchema#normalizedString"
                | "http://www.w3.org/2001/XMLSchema#token"
                | "http://www.w3.org/2001/XMLSchema#language"
                | "http://www.w3.org/2001/XMLSchema#anyURI"
        )
    {
        return Some(Literal::new_simple_literal(format!("string:{}", literal.value())).into());
    }
    match parse_numeric(literal, context)? {
        PythonNumericValue::Comparable(key) => Some(Literal::new_simple_literal(key.key()).into()),
        _ => None,
    }
}

pub(super) fn value_query(context: LexicalContext) -> Result<PreparedSparqlQuery> {
    let evaluator = SparqlEvaluator::new().with_custom_function(
        NamedNode::new_unchecked(LITERAL_VALUE_RELATION),
        move |args| match args {
            [Term::Literal(literal)] => value_key(literal, &context),
            _ => None,
        },
    );
    CompiledRule::with_literal_values(
        &LiteralRuleId::RdfsValueSubstitution.definition(),
        evaluator,
    )
    .map(CompiledRule::into_query)
}

pub(super) fn substitute(data: &mut impl Graph, context: &LexicalContext) -> Result<()> {
    // Equality involving Decimal and float signals FloatOperation even when
    // their exact keys differ. Scan scalar categories, never literal pairs.
    #[cfg(all(test, feature = "allocation-profiling"))]
    trace::mark("literal_scalar_scan_before", 0, (0, 0));
    let mut decimal = false;
    let mut float = false;
    let mut signaling_nan = false;
    let mut first_numeric = None; // bounded: one scalar literal, independent of graph size.
    let mut distinct_numeric = false;
    for quad in data.quads(None, None, None) {
        let Term::Literal(literal) = quad?.object else {
            continue;
        };
        let numeric = parse_numeric(&literal, context);
        signaling_nan |= matches!(numeric, Some(PythonNumericValue::SignalingNaN));
        if !matches!(numeric, None | Some(PythonNumericValue::Missing)) {
            if let Some(first) = &first_numeric {
                distinct_numeric |= first != &literal;
            } else {
                first_numeric = Some(literal.clone());
            }
            decimal |= literal.datatype().as_str() == "http://www.w3.org/2001/XMLSchema#decimal";
            float |= matches!(
                literal.datatype().as_str(),
                "http://www.w3.org/2001/XMLSchema#float"
                    | "http://www.w3.org/2001/XMLSchema#double"
            );
        }
    }
    if decimal && float {
        context.record_float_operation();
    }
    // The reference bypasses equality for literal identity. A lone sNaN
    // therefore signals only if another distinct numeric literal is present.
    if signaling_nan && distinct_numeric {
        context.record_invalid_operation();
        if context.traps_invalid_operation() {
            return Err(Error::Lexical(
                super::LexicalFailure::DecimalInvalidComparison,
            ));
        }
    }
    #[cfg(all(test, feature = "allocation-profiling"))]
    trace::mark("literal_scalar_scan_after", 0, (0, 0));
    let query = value_query(context.clone())?;
    #[cfg(all(test, feature = "allocation-profiling"))]
    trace::mark("literal_query_prepared", 0, (0, 0));
    let triples = data.inference_heads(query)?;
    #[cfg(all(test, feature = "allocation-profiling"))]
    trace::mark("literal_query_started", 0, (0, 0));
    let mut additions = Pending::default();
    for triple in triples {
        let triple = triple?;
        let quad = GeneralizedQuad::new(
            triple.subject,
            triple.predicate,
            triple.object,
            GraphName::DefaultGraph,
        );
        // Original RDFS one_time_rules uses destination.add directly, unlike
        // OWL Core.store_triple: literal predicates are valid substitutions.
        // Retain immutable query evaluation and only distinct destination delta.
        additions.publish(data, quad)?;
    }
    #[cfg(all(test, feature = "allocation-profiling"))]
    trace::mark("literal_collection_done", 0, additions.trace_counts());
    for quad in additions.into_heads() {
        data.insert_head(quad)?;
    }
    #[cfg(all(test, feature = "allocation-profiling"))]
    trace::mark("literal_publication_done", 0, (0, 0));
    Ok(())
}
