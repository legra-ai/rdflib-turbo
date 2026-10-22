//! RDFS one-time substitution uses Python value equality, not RDF term equality.

use oxigraph::model::{GraphName, Quad};
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults, SparqlEvaluator};

use super::backend::Graph;
use super::{PREFIXES, Pending};
use crate::{Error, Result};

const STRING_VALUE: &str = r#"
isLiteral(?value) && (
    LANG(?value) != "" ||
    DATATYPE(?value) IN (
        xsd:string, xsd:normalizedString, xsd:token, xsd:language, xsd:anyURI
    )
)
"#;

pub(super) fn string_query() -> Result<PreparedSparqlQuery> {
    // Both subqueries bind the same value key before joining. The evaluator can
    // hash-join equal values instead of comparing every pair of input literals.
    SparqlEvaluator::new()
        .parse_query(&format!(
            r#"{PREFIXES}
CONSTRUCT {{ ?subject ?predicate ?replacement }} WHERE {{
    {{ SELECT ?subject ?predicate ?key WHERE {{
        ?subject ?predicate ?value .
        FILTER ({STRING_VALUE})
        BIND (STR(?value) AS ?key)
        FILTER (BOUND(?key))
    }} }}
    {{ SELECT DISTINCT ?replacement ?key WHERE {{
        ?otherSubject ?otherPredicate ?value .
        FILTER ({STRING_VALUE})
        BIND (?value AS ?replacement)
        BIND (STR(?value) AS ?key)
        FILTER (BOUND(?key))
    }} }}
}}"#
        ))
        .map_err(|error| Error::Runtime(format!("Invalid literal substitution rule: {error}")))
}

pub(super) fn strings(data: &mut impl Graph) -> Result<()> {
    let QueryResults::Graph(triples) = data.query(string_query()?)? else {
        unreachable!("Literal substitution is a CONSTRUCT query")
    };
    let mut additions = Pending::default();
    for triple in triples {
        let triple = triple?;
        additions.push(
            data,
            Quad::new(
                triple.subject,
                triple.predicate,
                triple.object,
                GraphName::DefaultGraph,
            ),
        )?;
    }
    for quad in additions.0 {
        data.insert(quad)?;
    }
    Ok(())
}
