//! Entry-context filters prevent SPARQL reordering across their enclosing plan.
use crate::algebra::{AggregateExpression, Expression, GraphPattern, OrderExpression};

impl GraphPattern {
    pub(crate) fn contains_rdflib_scope(&self) -> bool {
        match self {
            Self::RdfLibFilter { .. } => true,
            Self::QuadPattern { .. }
            | Self::Path { .. }
            | Self::Graph { .. }
            | Self::Values { .. } => false,
            Self::Join { left, right, .. } | Self::Minus { left, right, .. } => {
                left.contains_rdflib_scope() || right.contains_rdflib_scope()
            }
            #[cfg(feature = "sep-0006")]
            Self::Lateral { left, right } => {
                left.contains_rdflib_scope() || right.contains_rdflib_scope()
            }
            Self::LeftJoin {
                left,
                right,
                expression,
                ..
            } => {
                left.contains_rdflib_scope()
                    || right.contains_rdflib_scope()
                    || expression.contains_rdflib_scope()
            }
            Self::Filter { inner, expression }
            | Self::Extend {
                inner, expression, ..
            } => inner.contains_rdflib_scope() || expression.contains_rdflib_scope(),
            Self::Union { inner } => inner.iter().any(Self::contains_rdflib_scope),
            Self::OrderBy { inner, expression } => {
                inner.contains_rdflib_scope()
                    || expression.iter().any(|order| match order {
                        OrderExpression::Asc(value) | OrderExpression::Desc(value) => {
                            value.contains_rdflib_scope()
                        }
                    })
            }
            Self::Project { inner, .. }
            | Self::Distinct { inner }
            | Self::Reduced { inner }
            | Self::Slice { inner, .. }
            | Self::Service { inner, .. } => inner.contains_rdflib_scope(),
            Self::Group {
                inner, aggregates, ..
            } => {
                inner.contains_rdflib_scope()
                    || aggregates.iter().any(|(_, aggregate)| match aggregate {
                        AggregateExpression::CountSolutions { .. } => false,
                        AggregateExpression::FunctionCall { expr, .. } => {
                            expr.contains_rdflib_scope()
                        }
                    })
            }
        }
    }
}

impl Expression {
    fn contains_rdflib_scope(&self) -> bool {
        match self {
            Self::NamedNode(_) | Self::Literal(_) | Self::Variable(_) | Self::Bound(_) => false,
            Self::Exists(pattern) => pattern.contains_rdflib_scope(),
            Self::Or(values)
            | Self::And(values)
            | Self::Coalesce(values)
            | Self::FunctionCall(_, values) => values.iter().any(Self::contains_rdflib_scope),
            Self::Equal(left, right)
            | Self::SameTerm(left, right)
            | Self::Greater(left, right)
            | Self::GreaterOrEqual(left, right)
            | Self::Less(left, right)
            | Self::LessOrEqual(left, right)
            | Self::Add(left, right)
            | Self::Subtract(left, right)
            | Self::Multiply(left, right)
            | Self::Divide(left, right) => {
                left.contains_rdflib_scope() || right.contains_rdflib_scope()
            }
            Self::UnaryPlus(value) | Self::UnaryMinus(value) | Self::Not(value) => {
                value.contains_rdflib_scope()
            }
            Self::If(condition, yes, no) => {
                condition.contains_rdflib_scope()
                    || yes.contains_rdflib_scope()
                    || no.contains_rdflib_scope()
            }
        }
    }
}
