use oxigraph::model::Term;
use serde::Deserialize;
use spargebra::term::TermPattern;
use sparopt::algebra::Expression;

use super::{pattern::Pattern, term::Atom};
use crate::{Error, Result};

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub(super) enum InputExpression {
    Atom {
        value: Atom,
    },
    Exists {
        pattern: Box<Pattern>,
    },
    Binary {
        operator: String,
        left: Box<Self>,
        right: Box<Self>,
    },
    Unary {
        operator: String,
        inner: Box<Self>,
    },
}

pub(super) fn constant(term: Term) -> Result<Expression> {
    match term {
        Term::NamedNode(value) => Ok(Expression::NamedNode(value)),
        Term::Literal(value) => Ok(Expression::Literal(value)),
        Term::BlankNode(_) => Err(Error::Invalid("Blank node expression".into())),
    }
}

impl InputExpression {
    pub(super) fn build(self) -> Result<Expression> {
        Ok(match self {
            Self::Atom { value } => match value.build()? {
                TermPattern::NamedNode(value) => Expression::NamedNode(value),
                TermPattern::Literal(value) => Expression::Literal(value),
                TermPattern::Variable(value) => Expression::Variable(value),
                _ => return Err(Error::Invalid("Blank node expression".into())),
            },
            Self::Exists { pattern } => Expression::Exists(Box::new(pattern.build()?)),
            Self::Unary { operator, inner } => {
                let inner = Box::new(inner.build()?);
                match operator.as_str() {
                    "!" => Expression::Not(inner),
                    "+" => Expression::UnaryPlus(inner),
                    "-" => Expression::UnaryMinus(inner),
                    _ => {
                        return Err(Error::Unsupported(format!(
                            "Prepared unary operator {operator}"
                        )));
                    }
                }
            }
            Self::Binary {
                operator,
                left,
                right,
            } => {
                let left = Box::new(left.build()?);
                let right = Box::new(right.build()?);
                match operator.as_str() {
                    "=" => Expression::Equal(left, right),
                    "!=" => Expression::Not(Box::new(Expression::Equal(left, right))),
                    ">" => Expression::Greater(left, right),
                    "<" => Expression::Less(left, right),
                    ">=" => Expression::GreaterOrEqual(left, right),
                    "<=" => Expression::LessOrEqual(left, right),
                    "+" => Expression::Add(left, right),
                    "-" => Expression::Subtract(left, right),
                    "*" => Expression::Multiply(left, right),
                    "/" => Expression::Divide(left, right),
                    "&&" => Expression::And(vec![*left, *right]),
                    "||" => Expression::Or(vec![*left, *right]),
                    _ => {
                        return Err(Error::Unsupported(format!(
                            "Prepared binary operator {operator}"
                        )));
                    }
                }
            }
        })
    }
}
