//! RDFS one-time substitution joins caller-profile value classes.
use oxigraph::model::{GraphName, Literal, NamedNode, Quad, Term};
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults, SparqlEvaluator};
use rdf_reasoning::{LITERAL_VALUE_RELATION, LiteralRuleId};

use super::Pending;
use super::backend::Graph;
use super::diagnostic_datatypes::{LexicalContext, PythonNumericValue, parse_numeric};
use super::shared::CompiledRule;
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
    let QueryResults::Solutions(rows) = data.query(value_query(context.clone())?)? else {
        unreachable!("Literal substitution uses a SELECT query")
    };
    let mut additions = Pending::default();
    for row in rows {
        let row = row?;
        let subject = row
            .get("__head_s")
            .expect("safe literal rule subject")
            .clone()
            .try_into()
            .map_err(|_| Error::Unsupported("Literal rule produced a non-RDF subject".into()))?;
        let predicate = row
            .get("__head_p")
            .expect("safe literal rule predicate")
            .clone()
            .try_into()
            .map_err(|_| Error::Unsupported("Literal rule produced a non-IRI predicate".into()))?;
        let object = row
            .get("__head_o")
            .expect("safe literal rule object")
            .clone();
        additions.push(
            data,
            Quad {
                subject,
                predicate,
                object,
                graph_name: GraphName::DefaultGraph,
            },
        )?;
    }
    for quad in additions.0 {
        data.insert(quad)?;
    }
    Ok(())
}
