use oxigraph::model::Variable;
use serde::Deserialize;
use spargebra::algebra::GraphPattern as SourcePattern;
use spargebra::term::NamedNodePattern;
use sparopt::algebra::{
    GraphPattern, JoinAlgorithm, LeftJoinAlgorithm, MinusAlgorithm, OrderExpression,
};

use super::expression::InputExpression;
use super::term::{Atom, triple};
use crate::{Error, Result};

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub(super) enum Pattern {
    Bgp {
        triples: Vec<[Atom; 3]>,
    },
    Join {
        left: Box<Self>,
        right: Box<Self>,
        lazy: bool,
    },
    Optional {
        left: Box<Self>,
        right: Box<Self>,
        expression: InputExpression,
        variables: SourceVariables,
    },
    Union {
        left: Box<Self>,
        right: Box<Self>,
    },
    Minus {
        left: Box<Self>,
        right: Box<Self>,
    },
    Project {
        inner: Box<Self>,
        variables: Vec<String>,
    },
    Extend {
        inner: Box<Self>,
        variable: String,
        expression: InputExpression,
    },
    Filter {
        inner: Box<Self>,
        expression: InputExpression,
        variables: SourceVariables,
        no_isolated_scope: bool,
    },
    Values {
        variables: Vec<String>,
        rows: Vec<Vec<Option<Atom>>>,
    },
    Orderby {
        inner: Box<Self>,
        expressions: Vec<Order>,
    },
    Distinct {
        inner: Box<Self>,
    },
    Reduced {
        inner: Box<Self>,
    },
    Slice {
        inner: Box<Self>,
        start: usize,
        length: Option<usize>,
    },
    Graph {
        inner: Box<Self>,
        name: Atom,
    },
}

#[derive(Deserialize)]
#[serde(tag = "state", rename_all = "lowercase", deny_unknown_fields)]
pub(super) enum SourceVariables {
    Absent,
    Present { keys: Vec<ScopeKey> },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub(super) enum ScopeKey {
    Variable {
        name: String,
    },
    Blank {
        label: String,
    },
    String {
        value: String,
    },
    Constant {
        term: super::super::pattern::InputTerm,
    },
}

impl SourceVariables {
    fn build(self) -> Result<Option<Vec<Variable>>> {
        match self {
            Self::Absent => Ok(None),
            Self::Present { keys } => keys
                .into_iter()
                .map(|key| match key {
                    ScopeKey::Variable { name } => Ok(Variable::new_unchecked(name)),
                    _ => Err(Error::Invalid(
                        "Prepared scope keys were not lowered".into(),
                    )),
                })
                .collect::<Result<Vec<_>>>()
                .map(Some),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Order {
    descending: bool,
    pub(super) expression: InputExpression,
}

impl Pattern {
    pub(super) fn build(self) -> Result<GraphPattern> {
        self.scoped(None)
    }

    fn scoped(self, graph: Option<NamedNodePattern>) -> Result<GraphPattern> {
        Ok(match self {
            Self::Bgp { triples } => {
                let mut source = SourcePattern::Bgp {
                    patterns: triples.into_iter().map(triple).collect::<Result<_>>()?,
                };
                if let Some(name) = graph {
                    source = SourcePattern::Graph {
                        inner: Box::new(source),
                        name,
                    };
                }
                GraphPattern::from_rdflib_algebra(&source)
            }
            Self::Join { left, right, lazy } => GraphPattern::Join {
                left: Box::new(left.scoped(graph.clone())?),
                right: Box::new(right.scoped(graph)?),
                algorithm: if lazy {
                    JoinAlgorithm::RdfLibLazy
                } else {
                    JoinAlgorithm::RdfLibIndependent
                },
            },
            Self::Optional {
                left,
                right,
                expression,
                variables,
            } => GraphPattern::LeftJoin {
                left: Box::new(left.scoped(graph.clone())?),
                right: Box::new(right.scoped(graph)?),
                expression: expression.build()?,
                algorithm: LeftJoinAlgorithm::RdfLib {
                    variables: variables.build()?,
                },
            },
            Self::Union { left, right } => GraphPattern::Union {
                inner: vec![left.scoped(graph.clone())?, right.scoped(graph)?],
            },
            Self::Minus { left, right } => GraphPattern::Minus {
                left: Box::new(left.scoped(graph.clone())?),
                right: Box::new(right.scoped(graph)?),
                algorithm: MinusAlgorithm::default(),
            },
            Self::Project { inner, variables } => GraphPattern::Project {
                inner: Box::new(inner.scoped(graph)?),
                variables: variables.into_iter().map(Variable::new_unchecked).collect(),
            },
            Self::Extend {
                inner,
                variable,
                expression,
            } => GraphPattern::Extend {
                inner: Box::new(inner.scoped(graph)?),
                variable: Variable::new_unchecked(variable),
                expression: expression.build()?,
            },
            Self::Filter {
                inner,
                expression,
                variables,
                no_isolated_scope,
            } => GraphPattern::RdfLibFilter {
                inner: Box::new(inner.scoped(graph)?),
                expression: expression.build()?,
                variables: variables.build()?,
                no_isolated_scope,
            },
            Self::Values { variables, rows } => GraphPattern::Values {
                variables: variables.into_iter().map(Variable::new_unchecked).collect(),
                bindings: rows
                    .into_iter()
                    .map(|row| {
                        row.into_iter()
                            .map(|value| value.map(Atom::ground).transpose())
                            .collect()
                    })
                    .collect::<Result<_>>()?,
            },
            Self::Orderby { inner, expressions } => GraphPattern::OrderBy {
                inner: Box::new(inner.scoped(graph)?),
                expression: expressions
                    .into_iter()
                    .map(|order| {
                        let expression = order.expression.build()?;
                        Ok(if order.descending {
                            OrderExpression::Desc(expression)
                        } else {
                            OrderExpression::Asc(expression)
                        })
                    })
                    .collect::<Result<_>>()?,
            },
            Self::Distinct { inner } => GraphPattern::Distinct {
                inner: Box::new(inner.scoped(graph)?),
            },
            Self::Reduced { inner } => GraphPattern::Reduced {
                inner: Box::new(inner.scoped(graph)?),
            },
            Self::Slice {
                inner,
                start,
                length,
            } => GraphPattern::Slice {
                inner: Box::new(inner.scoped(graph)?),
                start,
                length,
            },
            Self::Graph { inner, name } => inner.scoped(Some(name.name()?))?,
        })
    }
}
