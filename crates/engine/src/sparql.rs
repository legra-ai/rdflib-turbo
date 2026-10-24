use std::collections::{BTreeMap, HashSet};

use crate::functions::{Functions, NativeQuery};
use crate::model;
use oxigraph::model::*;
use spargebra::algebra::{
    AggregateExpression, Expression, GraphPattern, OrderExpression, PropertyPathExpression,
};
use spargebra::term::{NamedNodePattern, TermPattern};
use spargebra::{Query, SparqlParser};

use crate::graph::*;
use crate::path::Path;
use crate::{Error, GraphRead, OverlayGraph, Result};

#[cfg(test)]
#[path = "sparql_tests.rs"]
mod tests;

pub(crate) struct SparqlConstraint {
    pub node: Term,
    pub query: NativeQuery,
    pub messages: Vec<Term>,
    pub deactivated: bool,
    pub shapes_graph: bool,
    pub component: Option<NamedNode>,
    pub per_value: bool,
    bindings: Vec<(Variable, Term)>,
}

pub(crate) fn lower(
    graph: &(impl GraphRead + ?Sized),
    declaration: model::SparqlConstraint<Term>,
    path: Option<&Path>,
    namespaces: &[(String, String)],
) -> Result<SparqlConstraint> {
    let validator = model::SparqlValidator {
        id: declaration.id,
        query: model::SparqlSelectOrAsk::Select(declaration.select),
        messages: declaration.messages,
        annotations: declaration.annotations,
    };
    lower_validator(
        graph,
        validator,
        path,
        &[],
        declaration.deactivated,
        None,
        namespaces,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_validator(
    graph: &(impl GraphRead + ?Sized),
    declaration: model::SparqlValidator<Term>,
    path: Option<&Path>,
    bindings: &[(Variable, Term)],
    deactivated: bool,
    component: Option<NamedNode>,
    namespaces: &[(String, String)],
) -> Result<SparqlConstraint> {
    let (query, ask) = match declaration.query {
        model::SparqlSelectOrAsk::Select(query) => (query, false),
        model::SparqlSelectOrAsk::Ask(query) => (query, true),
    };
    let Term::Literal(text) = query.text else {
        return Err(Error::Invalid("sh:select requires an xsd:string".into()));
    };
    if text.datatype() != iri(XSD, "string").as_ref() {
        return Err(Error::Invalid("sh:select requires an xsd:string".into()));
    }
    let mut prebound: Vec<Variable> = ["this", "currentShape", "shapesGraph"]
        .iter()
        .map(|v| Variable::new_unchecked(*v))
        .collect();
    prebound.extend(bindings.iter().map(|(v, _)| v.clone()));
    if ask {
        prebound.push(Variable::new_unchecked("value"));
    }
    let mut parser = SparqlParser::new();
    for variable in &prebound {
        parser = parser.with_external_variable(variable.clone());
    }
    let mut parsed = parser
        .parse_query(&format!(
            "{}\n{}",
            prefixes(graph, query.prefixes, namespaces)?,
            text.value()
        ))
        .map_err(|e| Error::Invalid(format!("SPARQL parse error: {e}")))?;
    let mut visitor = ConstraintQuery {
        path,
        shapes_graph: false,
        prebound: &prebound,
    };
    if ask {
        let Query::Ask {
            mut pattern,
            dataset,
            base_iri,
        } = parsed
        else {
            return Err(Error::Invalid("sh:ask requires ASK".into()));
        };
        fn ask_projection(pattern: &mut GraphPattern, prebound: &[Variable]) {
            match pattern {
                GraphPattern::Project { variables, .. } => {
                    for variable in prebound {
                        if !variables.contains(variable) {
                            variables.push(variable.clone());
                        }
                    }
                }
                GraphPattern::Slice { inner, .. }
                | GraphPattern::Distinct { inner }
                | GraphPattern::Reduced { inner } => ask_projection(inner, prebound),
                _ => (),
            }
        }
        // Spargebra models ASK with an implicit SELECT * projection. It is not
        // a user subquery and must receive the ASK validator's prebindings.
        ask_projection(&mut pattern, &prebound);
        visitor.pattern(&mut pattern)?;
        // Select exactly one violation row when the ASK validator is false.
        parsed = Query::Select {
            pattern: GraphPattern::Filter {
                expr: Expression::Not(Box::new(Expression::Exists(Box::new(pattern)))),
                inner: Box::new(GraphPattern::default()),
            },
            dataset,
            base_iri,
        };
    } else {
        let Query::Select { pattern, .. } = &mut parsed else {
            return Err(Error::Invalid("sh:select requires SELECT".into()));
        };
        visitor.pattern(pattern)?;
    }
    let Query::Select {
        pattern, dataset, ..
    } = &mut parsed
    else {
        unreachable!()
    };
    if dataset.is_some() {
        return Err(Error::Unsupported("SPARQL FROM dataset clauses".into()));
    }

    let mut variables = Vec::new();
    pattern.on_in_scope_variable(|v| variables.push(v.clone()));
    for var in prebound.iter().cloned() {
        if !variables.contains(&var) {
            variables.push(var);
        }
    }
    *pattern = GraphPattern::Project {
        inner: Box::new(pattern.clone()),
        variables,
    };
    let messages = declaration.messages;
    if messages.iter().any(|m| !matches!(m, Term::Literal(_))) {
        return Err(Error::Invalid("sh:message requires literals".into()));
    }
    Ok(SparqlConstraint {
        node: declaration.id,
        query: NativeQuery::new(parsed),
        messages,
        deactivated,
        shapes_graph: visitor.shapes_graph,
        component,
        per_value: ask,
        bindings: bindings.to_vec(),
    })
}

pub(crate) fn prefixes(
    graph: &(impl GraphRead + ?Sized),
    mut pending: Vec<Term>,
    namespaces: &[(String, String)],
) -> Result<String> {
    let mut seen = HashSet::new();
    let mut prefixes = BTreeMap::new();
    while let Some(resource) = pending.pop() {
        if !seen.insert(resource.clone()) {
            continue;
        }
        let resource = crate::structural::prefix_resource(graph, resource)?;
        pending.extend(resource.imports);
        for declaration in resource.declarations {
            let (Term::Literal(prefix), Term::Literal(namespace)) =
                (declaration.prefix, declaration.namespace)
            else {
                return Err(Error::Invalid(
                    "Prefix declaration requires sh:prefix and sh:namespace literals".into(),
                ));
            };
            if prefix.datatype() != iri(XSD, "string").as_ref()
                || ![iri(XSD, "anyURI"), iri(XSD, "string")]
                    .iter()
                    .any(|d| namespace.datatype() == d.as_ref())
            {
                return Err(Error::Invalid(
                    "Invalid prefix declaration datatypes".into(),
                ));
            }
            let ns =
                NamedNode::new(namespace.value()).map_err(|e| Error::Invalid(e.to_string()))?;
            if let Some(old) = prefixes.insert(prefix.value().to_owned(), ns.clone())
                && old != ns
            {
                return Err(Error::Invalid(format!(
                    "Conflicting SPARQL prefix {}",
                    prefix.value()
                )));
            }
        }
    }
    for (prefix, namespace) in namespaces {
        prefixes
            .entry(prefix.clone())
            .or_insert(NamedNode::new(namespace).map_err(|e| Error::Invalid(e.to_string()))?);
    }
    Ok(prefixes
        .iter()
        .map(|(prefix, ns)| format!("PREFIX {prefix}: {ns}\n"))
        .collect())
}

impl Path {
    fn sparql(&self) -> PropertyPathExpression {
        use PropertyPathExpression as P;
        match self {
            Self::Predicate(p) => P::NamedNode(p.clone()),
            Self::Inverse(p) => P::Reverse(Box::new(p.sparql())),
            Self::ZeroOrMore(p) => P::ZeroOrMore(Box::new(p.sparql())),
            Self::OneOrMore(p) => P::OneOrMore(Box::new(p.sparql())),
            Self::ZeroOrOne(p) => P::ZeroOrOne(Box::new(p.sparql())),
            Self::Sequence(paths) | Self::Alternative(paths) => paths
                .iter()
                .map(Self::sparql)
                .reduce(|a, b| {
                    if matches!(self, Self::Sequence(_)) {
                        P::Sequence(Box::new(a), Box::new(b))
                    } else {
                        P::Alternative(Box::new(a), Box::new(b))
                    }
                })
                .expect("Compiled paths have at least two members"),
        }
    }
}

struct ConstraintQuery<'a> {
    path: Option<&'a Path>,
    shapes_graph: bool,
    prebound: &'a [Variable],
}

impl ConstraintQuery<'_> {
    fn variable(&mut self, v: &Variable) -> Result<()> {
        match v.as_str() {
            "shapesGraph" => self.shapes_graph = true,
            "PATH" => {
                return Err(Error::Invalid(
                    "$PATH is only valid in predicate position".into(),
                ));
            }
            _ => (),
        }
        Ok(())
    }
    fn term(&mut self, term: &TermPattern) -> Result<()> {
        if let TermPattern::Variable(v) = term {
            self.variable(v)?;
        }
        Ok(())
    }
    fn expression(&mut self, expr: &mut Expression) -> Result<()> {
        use Expression as E;
        match expr {
            E::Variable(v) | E::Bound(v) => self.variable(v)?,
            E::Exists(pattern) => self.pattern(pattern)?,
            E::Or(a, b)
            | E::And(a, b)
            | E::Equal(a, b)
            | E::SameTerm(a, b)
            | E::Greater(a, b)
            | E::GreaterOrEqual(a, b)
            | E::Less(a, b)
            | E::LessOrEqual(a, b)
            | E::Add(a, b)
            | E::Subtract(a, b)
            | E::Multiply(a, b)
            | E::Divide(a, b) => {
                self.expression(a)?;
                self.expression(b)?;
            }
            E::In(a, args) => {
                self.expression(a)?;
                for arg in args {
                    self.expression(arg)?;
                }
            }
            E::UnaryPlus(a) | E::UnaryMinus(a) | E::Not(a) => self.expression(a)?,
            E::If(a, b, c) => {
                self.expression(a)?;
                self.expression(b)?;
                self.expression(c)?;
            }
            E::Coalesce(args) | E::FunctionCall(_, args) => {
                for arg in args {
                    self.expression(arg)?;
                }
            }
            E::NamedNode(_) | E::Literal(_) => (),
        }
        Ok(())
    }
    fn assignment(&mut self, v: &Variable) -> Result<()> {
        if self.prebound.contains(v) {
            return Err(Error::ValidationFailure(format!(
                "SPARQL cannot rebind prebound variable {v}"
            )));
        }
        self.variable(v)
    }
    fn pattern(&mut self, pattern: &mut GraphPattern) -> Result<()> {
        use GraphPattern as G;
        match pattern {
            G::Lateral { .. } => return Err(Error::Unsupported("SPARQL LATERAL extension".into())),
            G::Minus { .. } => {
                return Err(Error::ValidationFailure(
                    "SPARQL constraints cannot use MINUS".into(),
                ));
            }
            G::Service { .. } => {
                return Err(Error::ValidationFailure(
                    "SPARQL constraints cannot use SERVICE".into(),
                ));
            }
            G::Values { .. } => {
                return Err(Error::ValidationFailure(
                    "SPARQL constraints cannot use VALUES".into(),
                ));
            }
            G::Bgp { patterns } => {
                let mut paths = Vec::new();
                let mut triples = Vec::new();
                for triple in std::mem::take(patterns) {
                    self.term(&triple.subject)?;
                    self.term(&triple.object)?;
                    if let NamedNodePattern::Variable(v) = &triple.predicate {
                        if v.as_str() == "PATH" {
                            let path = self.path.ok_or_else(|| {
                                Error::Invalid("$PATH requires a property shape".into())
                            })?;
                            paths.push(G::Path {
                                subject: triple.subject,
                                path: path.sparql(),
                                object: triple.object,
                            });
                            continue;
                        }
                        self.variable(v)?;
                    }
                    triples.push(triple);
                }
                *pattern = G::Bgp { patterns: triples };
                for path in paths {
                    *pattern = G::Join {
                        left: Box::new(std::mem::take(pattern)),
                        right: Box::new(path),
                    };
                }
            }
            G::Path {
                subject, object, ..
            } => {
                self.term(subject)?;
                self.term(object)?;
            }
            G::Join { left, right } | G::Union { left, right } => {
                self.pattern(left)?;
                self.pattern(right)?;
            }
            G::LeftJoin {
                left,
                right,
                expression,
            } => {
                self.pattern(left)?;
                self.pattern(right)?;
                if let Some(expr) = expression {
                    self.expression(expr)?;
                }
            }
            G::Filter { expr, inner } => {
                self.expression(expr)?;
                self.pattern(inner)?;
            }
            G::Graph { name, inner } => {
                if let NamedNodePattern::Variable(v) = name {
                    self.variable(v)?;
                }
                self.pattern(inner)?;
            }
            G::Extend {
                inner,
                variable,
                expression,
            } => {
                self.assignment(variable)?;
                self.expression(expression)?;
                self.pattern(inner)?;
            }
            G::OrderBy { inner, expression } => {
                for order in expression {
                    let (OrderExpression::Asc(expr) | OrderExpression::Desc(expr)) = order;
                    self.expression(expr)?;
                }
                self.pattern(inner)?;
            }
            G::Project { inner, variables }
            | G::ProjectWithSourceScope {
                inner, variables, ..
            } => {
                if !variables.iter().any(|v| v.as_str() == "this") {
                    return Err(Error::ValidationFailure(
                        "Every SELECT in a SPARQL constraint must project ?this".into(),
                    ));
                }
                for v in variables.iter() {
                    self.variable(v)?;
                }
                // These SHACL bindings are inputs of each scope, including
                // projections that do not expose them as query results.
                for variable in self.prebound.iter().cloned() {
                    if !variables.contains(&variable) {
                        variables.push(variable);
                    }
                }
                self.pattern(inner)?;
            }
            G::Distinct { inner } | G::Reduced { inner } | G::Slice { inner, .. } => {
                self.pattern(inner)?
            }
            G::Group {
                inner, aggregates, ..
            } => {
                for (variable, aggregate) in aggregates {
                    self.assignment(variable)?;
                    if let AggregateExpression::FunctionCall { expr, .. } = aggregate {
                        self.expression(expr)?;
                    }
                }
                self.pattern(inner)?;
            }
        }
        Ok(())
    }
}

pub(crate) struct SparqlRow {
    pub focus: Term,
    pub value: Option<Term>,
    pub path: Option<Term>,
    pub messages: Vec<Term>,
}

impl SparqlConstraint {
    #[allow(clippy::too_many_arguments)]
    pub fn evaluate(
        &self,
        data: &(impl GraphRead + ?Sized),
        shapes: &(impl GraphRead + ?Sized),
        shape: &Term,
        focus: &Term,
        messages: &[Term],
        value: Option<&Term>,
        functions: &Functions,
    ) -> Result<Vec<SparqlRow>> {
        if self.deactivated {
            return Ok(Vec::new());
        }
        let graph_name = BlankNode::default();
        if self.shapes_graph {
            let view = OverlayGraph::new(data, shapes, graph_name.clone().into())?;
            self.evaluate_on(&view, shape, focus, messages, value, functions, graph_name)
        } else {
            self.evaluate_on(data, shape, focus, messages, value, functions, graph_name)
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn evaluate_on(
        &self,
        data: &(impl GraphRead + ?Sized),
        shape: &Term,
        focus: &Term,
        messages: &[Term],
        value: Option<&Term>,
        functions: &Functions,
        graph_name: BlankNode,
    ) -> Result<Vec<SparqlRow>> {
        use oxigraph::sparql::QueryResults;
        let mut evaluation = self
            .query
            .prepare(functions)
            .on_queryable_dataset(data)
            .substitute_variable(Variable::new_unchecked("this"), focus.clone())
            .substitute_variable(Variable::new_unchecked("currentShape"), shape.clone())
            .substitute_variable(Variable::new_unchecked("shapesGraph"), graph_name);
        for (variable, term) in &self.bindings {
            evaluation = evaluation.substitute_variable(variable.clone(), term.clone());
        }
        if let Some(value) = value {
            evaluation =
                evaluation.substitute_variable(Variable::new_unchecked("value"), value.clone());
        }
        let QueryResults::Solutions(solutions) = evaluation.execute()? else {
            return Err(Error::Runtime("Expected SPARQL SELECT solutions".into()));
        };
        let mut rows = Vec::new();
        let mut seen = HashSet::new();
        for solution in solutions {
            let solution = solution?;
            if solution.get("failure") == Some(&Literal::from(true).into()) {
                return Err(Error::ValidationFailure(format!(
                    "SPARQL constraint {} returned ?failure=true",
                    self.node
                )));
            }
            let mut bindings: Vec<_> = solution
                .iter()
                .filter(|(v, _)| !["currentShape", "shapesGraph"].contains(&v.as_str()))
                .map(|(v, t)| (v.as_str().to_owned(), t.clone()))
                .collect();
            bindings.sort_by(|a, b| a.0.cmp(&b.0));
            if !seen.insert(bindings.clone()) {
                continue;
            }
            let mut result_messages = HashSet::new();
            for message in messages.iter().chain(&self.messages) {
                let Term::Literal(message) = message else {
                    unreachable!("Messages are checked at compile time");
                };
                let mut text = message.value().to_owned();
                for (variable, value) in &bindings {
                    let value = match value {
                        Term::NamedNode(v) => v.as_str(),
                        Term::BlankNode(v) => v.as_str(),
                        Term::Literal(v) => v.value(),
                    };
                    for sigil in ['?', '$'] {
                        text = text.replace(&format!("{{{sigil}{variable}}}"), value);
                    }
                }
                // pySHACL emits interpolated messages as untagged string literals.
                result_messages.insert(Literal::new_simple_literal(text).into());
            }
            rows.push(SparqlRow {
                focus: solution
                    .get("this")
                    .cloned()
                    .unwrap_or_else(|| focus.clone()),
                value: solution.get("value").cloned(),
                path: solution.get("path").cloned(),
                messages: result_messages.into_iter().collect(),
            });
        }
        Ok(rows)
    }
}
