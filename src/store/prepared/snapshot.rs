//! Typed prepared-algebra snapshots and their translation into native queries.

use oxigraph::model::Variable;
use oxigraph::sparql::{PreparedSparqlQuery, SparqlEvaluator};
use serde::Deserialize;
use spareval::RdfLibPreparedAlgebra;
use spargebra::Query;
use spargebra::algebra::QueryDataset;
use sparopt::algebra::GraphPattern;

use super::super::operations::invalid;
use super::pattern::Pattern;
use super::term::{self, Atom};
use super::{blank_nodes, census, expression};
use crate::Result;

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

pub(in crate::store) fn prepare<'a>(
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
#[path = "snapshot_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "snapshot_metadata_tests.rs"]
mod metadata_tests;
