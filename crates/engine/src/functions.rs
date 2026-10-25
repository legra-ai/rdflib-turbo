//! Native SHACL functions and query preparation with per-evaluation registration.

use spareval::ScopedCustomFunction;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use crate::model;
use bigdecimal::BigDecimal;
use oxigraph::model::*;
use oxigraph::sparql::QueryResults;
use spargebra::{Query, SparqlParser, algebra::GraphPattern};

use crate::graph::*;
use crate::{Error, GraphRead, Result};

#[cfg(test)]
#[path = "functions_tests.rs"]
mod tests;

mod query;
pub(crate) use query::NativeQuery;
#[cfg(test)]
use query::{Checked, NativePrepared};

pub(crate) type FunctionErrors = Arc<Mutex<Option<Error>>>;

pub(crate) struct Parameter {
    pub path: NamedNode,
    pub variable: Variable,
    pub optional: bool,
}

pub(crate) fn read_parameters(
    graph: &(impl GraphRead + ?Sized),
    node: &Term,
) -> Result<Vec<model::Parameter<Term>>> {
    values(graph, node, "parameter")?
        .into_iter()
        .map(|p| {
            let path = crate::structural::named_role(
                one(graph, &p, "path")?
                    .ok_or_else(|| Error::Invalid("SHACL parameter requires sh:path".into()))?,
                "Parameter path",
            )?;
            let mut parameter = model::Parameter::new(p.clone(), path);
            parameter.optional = boolean(graph, &p, "optional")?;
            parameter.order = one(graph, &p, "order")?;
            // Argument constraints are annotations, not executable shapes (ADR 0010).
            // Their original triples remain in the source graph.
            parameter.names = values(graph, &p, "name")?;
            parameter.descriptions = values(graph, &p, "description")?;
            Ok(parameter)
        })
        .collect()
}

pub(crate) fn lower_parameters(
    declarations: Vec<model::Parameter<Term>>,
) -> Result<Vec<Parameter>> {
    let mut parameters = Vec::new();
    for declaration in declarations {
        let path = declaration.path;
        let name = path.as_str().rsplit(['#', '/']).next().unwrap();
        let variable = Variable::new(name).map_err(|e| Error::Invalid(e.to_string()))?;
        let order = declaration
            .order
            .map(|term| match term {
                Term::Literal(l) => l
                    .value()
                    .parse::<BigDecimal>()
                    .map_err(|e| Error::Invalid(e.to_string())),
                _ => Err(Error::Invalid("Parameter order must be numeric".into())),
            })
            .transpose()?;
        parameters.push((
            order,
            Parameter {
                path,
                variable,
                optional: declaration.optional,
            },
        ));
    }
    let ordered = parameters.iter().all(|(o, _)| o.is_some());
    parameters.sort_by(|(a, x), (b, y)| {
        if ordered {
            a.cmp(b)
        } else {
            x.variable.as_str().cmp(y.variable.as_str())
        }
    });
    Ok(parameters.into_iter().map(|(_, p)| p).collect())
}

/// Make externally supplied variables available through the query's projection.
pub(crate) fn project_bindings(query: &mut Query, bindings: &[Variable]) {
    fn extend(pattern: &mut GraphPattern, bindings: &[Variable]) {
        match pattern {
            GraphPattern::Project { variables, .. } => {
                for v in bindings {
                    if !variables.contains(v) {
                        variables.push(v.clone());
                    }
                }
            }
            GraphPattern::Distinct { inner }
            | GraphPattern::Reduced { inner }
            | GraphPattern::Slice { inner, .. }
            | GraphPattern::OrderBy { inner, .. } => extend(inner, bindings),
            _ => {
                let mut variables = Vec::new();
                pattern.on_in_scope_variable(|v| variables.push(v.clone()));
                for v in bindings {
                    if !variables.contains(v) {
                        variables.push(v.clone());
                    }
                }
                *pattern = GraphPattern::Project {
                    inner: Box::new(pattern.clone()),
                    variables,
                };
            }
        }
    }
    match query {
        Query::Select { pattern, .. }
        | Query::Ask { pattern, .. }
        | Query::Construct { pattern, .. }
        | Query::Describe { pattern, .. } => extend(pattern, bindings),
    }
}

struct Function {
    query: NativeQuery,
    parameters: Vec<Parameter>,
    output: Option<Variable>,
}

#[derive(Clone, Default)]
pub(crate) struct Functions(Arc<HashMap<NamedNode, Function>>);

impl Functions {
    pub fn compile(
        graph: &(impl GraphRead + ?Sized),
        namespaces: &[(String, String)],
    ) -> Result<Self> {
        let mut functions = HashMap::new();
        for node in subjects(
            graph,
            &iri(RDF, "type"),
            Some(&iri(SH, "SPARQLFunction").into()),
        )? {
            let name = crate::structural::named_role(node.clone(), "SHACL function")?;
            let select = one(graph, &node, "select")?;
            let ask = one(graph, &node, "ask")?;
            let query = match (select, ask) {
                (Some(text @ Term::Literal(_)), None) => {
                    model::SparqlSelectOrAsk::Select(crate::structural::query(graph, &node, text)?)
                }
                (None, Some(text @ Term::Literal(_))) => {
                    model::SparqlSelectOrAsk::Ask(crate::structural::query(graph, &node, text)?)
                }
                _ => {
                    return Err(Error::Invalid(format!(
                        "Function {node} requires exactly one sh:select or sh:ask"
                    )));
                }
            };
            let mut declaration = model::ShaclFunction::new(name, query);
            declaration.parameters = read_parameters(graph, &node)?;
            let name = declaration.id;
            let parameters = lower_parameters(declaration.parameters)?;
            let (query, is_ask) = match declaration.query {
                model::SparqlSelectOrAsk::Select(query) => (query, false),
                model::SparqlSelectOrAsk::Ask(query) => (query, true),
            };
            let Term::Literal(text) = query.text else {
                unreachable!("Function query literal checked by reader")
            };
            let mut query = SparqlParser::new()
                .parse_query(&format!(
                    "{}\n{}",
                    crate::sparql::prefixes(graph, query.prefixes, namespaces)?,
                    text.value()
                ))
                .map_err(|e| Error::Invalid(format!("SPARQL function parse error: {e}")))?;
            let output = match &query {
                Query::Select { pattern, .. } if !is_ask => {
                    let mut first = None;
                    pattern.on_in_scope_variable(|v| {
                        if first.is_none() {
                            first = Some(v.clone());
                        }
                    });
                    Some(first.ok_or_else(|| {
                        Error::Invalid(format!("Function {node} must select a result variable"))
                    })?)
                }
                Query::Ask { .. } if is_ask => None,
                _ => {
                    return Err(Error::Invalid(format!(
                        "Function {node} query kind does not match its SHACL declaration"
                    )));
                }
            };
            project_bindings(
                &mut query,
                &parameters
                    .iter()
                    .map(|p| p.variable.clone())
                    .collect::<Vec<_>>(),
            );
            functions.insert(
                name,
                Function {
                    query: NativeQuery::new(query),
                    parameters,
                    output,
                },
            );
        }
        Ok(Self(Arc::new(functions)))
    }

    pub fn contains(&self, name: &NamedNode) -> bool {
        self.0.contains_key(name)
    }

    fn callbacks<'a, G: GraphRead + ?Sized>(
        &self,
        data: &'a G,
        depth: usize,
        errors: FunctionErrors,
    ) -> HashMap<NamedNode, ScopedCustomFunction<'a>> {
        self.0
            .keys()
            .map(|name| {
                let functions = self.clone();
                let name = name.clone();
                let errors = errors.clone();
                let function = name.clone();
                let callback: ScopedCustomFunction<'a> = Rc::new(move |args| {
                    if errors
                        .lock()
                        .expect("function error mutex poisoned")
                        .is_some()
                    {
                        return None;
                    }
                    match functions.call_inner(&function, args, data, depth + 1) {
                        Ok(value) => value,
                        Err(error) => {
                            *errors.lock().expect("function error mutex poisoned") = Some(error);
                            None
                        }
                    }
                });
                (name, callback)
            })
            .collect()
    }

    pub fn call(
        &self,
        name: &NamedNode,
        args: &[Term],
        data: &(impl GraphRead + ?Sized),
    ) -> Result<Option<Term>> {
        self.call_inner(name, args, data, 0)
    }

    fn call_inner(
        &self,
        name: &NamedNode,
        args: &[Term],
        data: &(impl GraphRead + ?Sized),
        depth: usize,
    ) -> Result<Option<Term>> {
        if depth >= 64 {
            return Err(Error::Runtime(
                "SHACL function recursion limit exceeded".into(),
            ));
        }
        let function = self
            .0
            .get(name)
            .ok_or_else(|| Error::Invalid(format!("Unknown SHACL function {name}")))?;
        if args.len() > function.parameters.len()
            || function
                .parameters
                .iter()
                .skip(args.len())
                .any(|p| !p.optional)
        {
            return Err(Error::Invalid(format!(
                "Wrong argument count for SHACL function {name}"
            )));
        }
        let mut prepared = function.query.prepare(self);
        prepared.depth = depth;
        let mut query = prepared.on_queryable_dataset(data);
        for (p, arg) in function.parameters.iter().zip(args) {
            query = query.substitute_variable(p.variable.clone(), arg.clone());
        }
        match query.execute()? {
            QueryResults::Boolean(value) => Ok(Some(Literal::from(value).into())),
            QueryResults::Solutions(mut rows) => Ok(rows
                .next()
                .transpose()?
                .and_then(|row| row.get(function.output.as_ref().unwrap()).cloned())),
            _ => Err(Error::Invalid(
                "SHACL function must return SELECT or ASK results".into(),
            )),
        }
    }
}

/// Execute a query with SHACL functions declared in the supplied shapes graph.
pub fn evaluate_query<'a>(
    data: &'a (impl GraphRead + ?Sized),
    shapes: &(impl GraphRead + ?Sized),
    query: &str,
    namespaces: &[(String, String)],
) -> Result<QueryResults<'a>> {
    let functions = Functions::compile(shapes, namespaces)?;
    let parsed = SparqlParser::new()
        .parse_query(query)
        .map_err(|e| Error::Invalid(format!("SPARQL parse error: {e}")))?;
    NativeQuery::new(parsed)
        .prepare(&functions)
        .on_queryable_dataset(data)
        .execute()
}

#[cfg(test)]
#[path = "functions_borrowed_tests.rs"]
mod borrowed_tests;
