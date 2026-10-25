//! Query-scoped SHACL callbacks and lazy error propagation.
use super::{FunctionErrors, Functions};
use crate::sparql_regex::RegexFunctions;
use crate::{GraphRead, QueryGraph, Result};
use oxigraph::model::*;
use oxigraph::sparql::{
    PreparedSparqlQuery, QueryEvaluationError, QueryResults, QuerySolutionIter, QueryTripleIter,
    SparqlEvaluator,
};
use spargebra::Query;
use std::sync::{Arc, Mutex};

pub(crate) struct NativeQuery {
    pub parsed: Query,
    prepared: PreparedSparqlQuery,
    regex: RegexFunctions,
}

impl NativeQuery {
    pub fn new(mut parsed: Query) -> Self {
        let regex = RegexFunctions::lower(&mut parsed);
        Self {
            prepared: SparqlEvaluator::new().for_query(parsed.clone()),
            parsed,
            regex,
        }
    }

    pub fn prepare(&self, functions: &Functions) -> NativePrepared {
        let errors = Arc::new(Mutex::new(None));
        NativePrepared {
            query: if self.regex.required() {
                self.regex
                    .register(SparqlEvaluator::new(), errors.clone())
                    .for_query(self.parsed.clone())
            } else {
                self.prepared.clone()
            },
            errors: Some(errors),
            functions: functions.clone(),
            depth: 0,
        }
    }
}

pub(crate) struct NativePrepared {
    pub(super) query: PreparedSparqlQuery,
    pub(super) errors: Option<FunctionErrors>,
    pub(super) functions: Functions,
    pub(super) depth: usize,
}

impl NativePrepared {
    pub fn on_queryable_dataset<G: GraphRead + ?Sized>(self, data: &G) -> NativeBound<'_, G> {
        NativeBound {
            prepared: self,
            data,
            bindings: Vec::new(),
        }
    }
}

pub(crate) struct NativeBound<'a, G: GraphRead + ?Sized> {
    prepared: NativePrepared,
    data: &'a G,
    bindings: Vec<(Variable, Term)>,
}

impl<'a, G: GraphRead + ?Sized> NativeBound<'a, G> {
    pub fn substitute_variable(mut self, variable: Variable, term: impl Into<Term>) -> Self {
        self.bindings.push((variable, term.into()));
        self
    }

    pub fn execute(self) -> Result<QueryResults<'a>> {
        let errors = self.prepared.errors.clone().unwrap_or_default();
        let callbacks = self
            .prepared
            .functions
            .callbacks(self.data, self.prepared.depth, errors);
        let mut query = self
            .prepared
            .query
            .on_queryable_dataset(QueryGraph::with_functions(self.data, callbacks));
        for (variable, value) in self.bindings {
            query = query.substitute_variable(variable, value);
        }
        let result = query.execute()?;
        let Some(errors) = self.prepared.errors else {
            return Ok(result);
        };
        if let Some(error) = errors.lock().expect("function error mutex poisoned").take() {
            return Err(error);
        }
        Ok(match result {
            QueryResults::Solutions(rows) => QueryResults::Solutions(QuerySolutionIter::new(
                rows.variables().to_vec().into(),
                Checked {
                    inner: rows,
                    errors,
                    done: false,
                },
            )),
            QueryResults::Graph(triples) => QueryResults::Graph(QueryTripleIter::new(Checked {
                inner: triples,
                errors,
                done: false,
            })),
            QueryResults::Boolean(value) => QueryResults::Boolean(value),
        })
    }
}

pub(super) struct Checked<I> {
    pub(super) inner: I,
    pub(super) errors: FunctionErrors,
    pub(super) done: bool,
}

impl<T, I: Iterator<Item = std::result::Result<T, QueryEvaluationError>>> Iterator for Checked<I> {
    type Item = std::result::Result<T, QueryEvaluationError>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let next = self.inner.next();
        if let Some(error) = self
            .errors
            .lock()
            .expect("function error mutex poisoned")
            .take()
        {
            self.done = true;
            return Some(Err(QueryEvaluationError::Unexpected(Box::new(error))));
        }
        self.done = next.is_none();
        next
    }
}
