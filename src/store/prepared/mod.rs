//! Typed snapshots of prepared RDFLib algebra. No Python objects cross a worker boundary.
mod blank_nodes;
mod census;
mod expression;
mod pattern;
mod term;

use oxigraph::model::Variable;
use oxigraph::sparql::{PreparedSparqlQuery, SparqlEvaluator};
use serde::Deserialize;
use spareval::RdfLibPreparedAlgebra;
use spargebra::Query;
use spargebra::algebra::QueryDataset;
use sparopt::algebra::GraphPattern;

use super::operations::invalid;
use crate::Result;
use pattern::Pattern;
use term::Atom;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    form: Form,
    base: Option<String>,
    pattern: Pattern,
    template: Vec<[Atom; 3]>,
    describe: Vec<Atom>,
    dataset: Vec<Clause>,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Form {
    Select,
    Ask,
    Construct,
    Describe,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Clause {
    named: bool,
    iri: String,
}

pub(super) fn prepare<'a>(
    source: &str,
    union: bool,
    reserved: impl IntoIterator<Item = &'a str>,
) -> Result<(PreparedSparqlQuery, &'static str)> {
    let mut snapshot: Snapshot = serde_json::from_str(source).map_err(invalid)?;
    let mut used_variables =
        census::variables(&snapshot.pattern, &snapshot.describe, &snapshot.template);
    used_variables.extend(reserved.into_iter().map(str::to_owned));
    let mut blanks = blank_nodes::QueryBlanks::new(used_variables);
    blanks.pattern(&mut snapshot.pattern)?;
    let mut used_variables = blanks.into_used();
    let mut plan = snapshot.pattern.build()?;
    if matches!(snapshot.form, Form::Describe) && !snapshot.describe.is_empty() {
        let mut variables = Vec::new();
        for (index, atom) in snapshot.describe.into_iter().enumerate() {
            match atom {
                Atom::Variable { variable } => variables.push(Variable::new_unchecked(variable)),
                Atom::Term { term } => {
                    let mut candidate = index;
                    let name = loop {
                        let name = format!("__describe_{candidate}");
                        if used_variables.insert(name.clone()) {
                            break name;
                        }
                        candidate += 1;
                    };
                    let variable = Variable::new_unchecked(name);
                    let expression = expression::constant(term.term()?)?;
                    plan = GraphPattern::Extend {
                        inner: Box::new(plan),
                        variable: variable.clone(),
                        expression,
                    };
                    variables.push(variable);
                }
            }
        }
        plan = GraphPattern::Project {
            inner: Box::new(plan),
            variables,
        };
    }
    let algebra = RdfLibPreparedAlgebra::new(plan);
    let pattern = algebra.source().clone();
    let base_iri = snapshot
        .base
        .map(|base| base.parse().map_err(invalid))
        .transpose()?;
    let dataset = if snapshot.dataset.is_empty() {
        None
    } else {
        let mut default = Vec::new();
        let mut named = Vec::new();
        for clause in snapshot.dataset {
            let iri = oxigraph::model::NamedNode::new(clause.iri).map_err(invalid)?;
            if clause.named {
                named.push(iri);
            } else {
                default.push(iri);
            }
        }
        Some(QueryDataset {
            default,
            named: Some(named),
        })
    };
    let kind = if matches!(snapshot.form, Form::Describe) {
        "DESCRIBE"
    } else {
        "CONSTRUCT"
    };
    let query = match snapshot.form {
        Form::Select => Query::Select {
            pattern,
            dataset,
            base_iri,
        },
        Form::Ask => Query::Ask {
            pattern,
            dataset,
            base_iri,
        },
        Form::Describe => Query::Describe {
            pattern,
            dataset,
            base_iri,
        },
        Form::Construct => Query::Construct {
            template: snapshot
                .template
                .into_iter()
                .map(term::triple)
                .collect::<Result<_>>()?,
            pattern,
            dataset,
            base_iri,
        },
    };
    let mut query = SparqlEvaluator::new()
        .for_query(query)
        .with_rdflib_prepared_algebra(algebra);
    if union && query.dataset().is_default_dataset() {
        query.dataset_mut().set_default_graph_as_union();
    }
    Ok((query, kind))
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxigraph::model::{GraphName, Literal, NamedNode, Quad};
    use oxigraph::sparql::QueryResults;
    use oxigraph::store::Store;

    #[derive(Deserialize)]
    struct Case {
        name: String,
        snapshot: String,
        count: usize,
        source: Option<String>,
        bindings: Option<String>,
        data: Option<Vec<[String; 3]>>,
    }

    #[test]
    fn prepared_forms_and_supplied_metadata_use_owned_native_algebra() {
        let cases: Vec<Case> = serde_json::from_str(include_str!(
            "../../../tests/fixtures/prepared_queries.json"
        ))
        .unwrap();
        for case in cases {
            let store = Store::new().unwrap();
            // bounded: three explicit triples in each synthetic query fixture.
            let triples = case.data.unwrap_or_else(|| {
                vec![
                    ["a".into(), "p".into(), "A".into()],
                    ["b".into(), "p".into(), "B".into()],
                    ["a".into(), "q".into(), "yes".into()],
                ]
            });
            for [subject, predicate, object] in triples {
                store
                    .insert(
                        Quad::new(
                            NamedNode::new(format!("urn:{subject}")).unwrap(),
                            NamedNode::new(format!("urn:{predicate}")).unwrap(),
                            Literal::new_simple_literal(object),
                            GraphName::DefaultGraph,
                        )
                        .as_ref(),
                    )
                    .unwrap();
            }
            let bindings =
                super::super::bindings::InitialBindings::parse(case.bindings.as_deref()).unwrap();
            let (query, _) = prepare(&case.snapshot, false, bindings.names()).unwrap();
            let query = bindings.apply(query).unwrap();
            let actual = match query.on_store(&store).execute().unwrap() {
                QueryResults::Boolean(value) => usize::from(value),
                QueryResults::Solutions(values) => values
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .unwrap()
                    .len(),
                QueryResults::Graph(values) => values
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .unwrap()
                    .len(),
            };
            assert_eq!(actual, case.count, "{}", case.name);
            if let Some(source) = &case.source {
                let query = SparqlEvaluator::new()
                    .with_rdflib_query_context()
                    .parse_query(source)
                    .unwrap();
                let query = super::super::bindings::apply(query, case.bindings.as_deref()).unwrap();
                let actual = match query.on_store(&store).execute().unwrap() {
                    QueryResults::Boolean(value) => usize::from(value),
                    QueryResults::Solutions(values) => values
                        .collect::<std::result::Result<Vec<_>, _>>()
                        .unwrap()
                        .len(),
                    QueryResults::Graph(values) => values
                        .collect::<std::result::Result<Vec<_>, _>>()
                        .unwrap()
                        .len(),
                };
                assert_eq!(actual, case.count, "source {}", case.name);
            }
        }
    }
}

#[cfg(test)]
mod metadata_tests {
    use super::*;
    use oxigraph::store::Store;

    #[test]
    fn prepared_metadata_cannot_be_omitted_or_rebound_to_another_query() {
        let missing = r#"{"form":"select","base":null,"pattern":{"kind":"optional","left":{"kind":"bgp","triples":[]},"right":{"kind":"bgp","triples":[]},"expression":{"kind":"atom","value":{"term":{"type":"literal","value":"true","datatype":"http://www.w3.org/2001/XMLSchema#boolean"}}}},"template":[],"describe":[],"dataset":[]}"#;
        assert!(
            prepare(missing, false, std::iter::empty()).is_err(),
            "missing source-vars metadata is not absent source-vars metadata"
        );
        let plan = RdfLibPreparedAlgebra::new(GraphPattern::empty_singleton());
        let other = spargebra::SparqlParser::new()
            .parse_query("SELECT ?s WHERE {?s <urn:p> ?o}")
            .unwrap();
        let query = SparqlEvaluator::new()
            .for_query(other)
            .with_rdflib_prepared_algebra(plan);
        assert!(query.on_store(&Store::new().unwrap()).execute().is_err());
    }
    #[test]
    fn prepared_query_configuration_is_rejected_for_update_before_mutations() {
        let configured = spareval::QueryEvaluator::new().with_rdflib_prepared_algebra(
            RdfLibPreparedAlgebra::new(GraphPattern::empty_singleton()),
        );
        let pattern = spargebra::algebra::GraphPattern::Project {
            inner: Box::new(spargebra::algebra::GraphPattern::Bgp {
                patterns: Vec::new(),
            }),
            variables: Vec::new(),
        };
        let dataset = oxigraph::model::Dataset::new();
        let result = configured
            .prepare_delete_insert(Vec::new(), Vec::new(), None, None, &pattern)
            .execute(&dataset);
        assert!(
            result.is_err(),
            "a prepared query plan must never select an UPDATE WHERE plan"
        );
    }
}
