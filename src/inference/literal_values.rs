//! RDFS one-time substitution uses Python value equality, not RDF term equality.

use oxigraph::model::{GraphName, Quad};
use oxigraph::sparql::{QueryResults, SparqlEvaluator};

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

pub(super) fn strings(data: &mut impl Graph) -> Result<()> {
    // Both subqueries bind the same value key before joining. The evaluator can
    // hash-join equal values instead of comparing every pair of input literals.
    let query = SparqlEvaluator::new()
        .parse_query(&format!(
            r#"{PREFIXES}
CONSTRUCT {{ ?subject ?predicate ?replacement }} WHERE {{
    {{ SELECT ?subject ?predicate ?key WHERE {{
        ?subject ?predicate ?value .
        FILTER ({STRING_VALUE})
        BIND (STR(?value) AS ?key)
    }} }}
    {{ SELECT DISTINCT ?replacement ?key WHERE {{
        ?otherSubject ?otherPredicate ?value .
        FILTER ({STRING_VALUE})
        BIND (?value AS ?replacement)
        BIND (STR(?value) AS ?key)
    }} }}
}}"#
        ))
        .map_err(|error| Error::Runtime(format!("Invalid literal substitution rule: {error}")))?;
    let QueryResults::Graph(triples) = data.query(query)? else {
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
        data.insert(quad);
    }
    Ok(())
}
