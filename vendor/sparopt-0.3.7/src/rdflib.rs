//! Original-node eligibility mirrors RDFLib's post-order algebra analysis.
use spargebra::algebra::{AggregateExpression, Expression, GraphPattern, OrderExpression};

/// A Join always blocks its parent, even when that Join is itself lazy.
/// BGP leaves do not block: their later artificial joins must remain distinct.
pub(crate) fn lazy_eligible(pattern: &GraphPattern) -> bool {
    match pattern {
        GraphPattern::Join { .. } | GraphPattern::Distinct { .. } | GraphPattern::Slice { .. } => {
            false
        }
        GraphPattern::Bgp { .. } | GraphPattern::Path { .. } | GraphPattern::Values { .. } => true,
        GraphPattern::LeftJoin {
            left,
            right,
            expression,
        } => {
            lazy_eligible(left)
                && lazy_eligible(right)
                && expression.as_ref().is_none_or(expression_eligible)
        }
        GraphPattern::Union { left, right } | GraphPattern::Minus { left, right } => {
            lazy_eligible(left) && lazy_eligible(right)
        }
        #[cfg(feature = "sep-0006")]
        GraphPattern::Lateral { left, right } => lazy_eligible(left) && lazy_eligible(right),
        GraphPattern::Filter { inner, expr } => lazy_eligible(inner) && expression_eligible(expr),
        GraphPattern::Extend {
            inner, expression, ..
        } => lazy_eligible(inner) && expression_eligible(expression),
        GraphPattern::Graph { inner, .. }
        | GraphPattern::Project { inner, .. }
        | GraphPattern::Reduced { inner }
        | GraphPattern::Service { inner, .. } => lazy_eligible(inner),
        GraphPattern::OrderBy { inner, expression } => {
            lazy_eligible(inner)
                && expression.iter().all(|order| {
                    let (OrderExpression::Asc(expression) | OrderExpression::Desc(expression)) =
                        order;
                    expression_eligible(expression)
                })
        }
        GraphPattern::Group {
            inner, aggregates, ..
        } => {
            lazy_eligible(inner)
                && aggregates.iter().all(|(_, aggregate)| match aggregate {
                    AggregateExpression::CountSolutions { .. } => true,
                    AggregateExpression::FunctionCall { expr, .. } => expression_eligible(expr),
                })
        }
    }
}

fn expression_eligible(expression: &Expression) -> bool {
    match expression {
        Expression::Exists(inner) => lazy_eligible(inner),
        Expression::NamedNode(_)
        | Expression::Literal(_)
        | Expression::Variable(_)
        | Expression::Bound(_) => true,
        Expression::Or(a, b)
        | Expression::And(a, b)
        | Expression::Equal(a, b)
        | Expression::SameTerm(a, b)
        | Expression::Greater(a, b)
        | Expression::GreaterOrEqual(a, b)
        | Expression::Less(a, b)
        | Expression::LessOrEqual(a, b)
        | Expression::Add(a, b)
        | Expression::Subtract(a, b)
        | Expression::Multiply(a, b)
        | Expression::Divide(a, b) => expression_eligible(a) && expression_eligible(b),
        Expression::In(left, right) => {
            expression_eligible(left) && right.iter().all(expression_eligible)
        }
        Expression::UnaryPlus(inner) | Expression::UnaryMinus(inner) | Expression::Not(inner) => {
            expression_eligible(inner)
        }
        Expression::If(a, b, c) => {
            expression_eligible(a) && expression_eligible(b) && expression_eligible(c)
        }
        Expression::Coalesce(args) | Expression::FunctionCall(_, args) => {
            args.iter().all(expression_eligible)
        }
    }
}

/// Match RDFLib `_addVars` on original algebra, including hidden Project names.
/// Extend expressions and relational-expression operands do not add scope.
pub(crate) fn variables(pattern: &GraphPattern) -> Vec<oxrdf::Variable> {
    let mut variables = std::collections::BTreeSet::new(); // bounded: this query's finite variable vocabulary.
    graph_variables(pattern, &mut variables);
    variables.into_iter().collect()
}

type Variables = std::collections::BTreeSet<oxrdf::Variable>;

fn graph_variables(pattern: &GraphPattern, output: &mut Variables) {
    match pattern {
        // RDFLib's ToMultiSet stores VALUES names in row-dictionary keys;
        // _addVars does not traverse those keys.
        GraphPattern::Values { .. } => {}
        GraphPattern::Bgp { .. } | GraphPattern::Path { .. } => {
            pattern.on_in_scope_variable(|variable| {
                output.insert(variable.clone());
            });
        }
        GraphPattern::Join { left, right }
        | GraphPattern::Union { left, right }
        | GraphPattern::Minus { left, right } => {
            graph_variables(left, output);
            graph_variables(right, output);
        }
        #[cfg(feature = "sep-0006")]
        GraphPattern::Lateral { left, right } => {
            graph_variables(left, output);
            graph_variables(right, output);
        }
        GraphPattern::LeftJoin {
            left,
            right,
            expression,
        } => {
            graph_variables(left, output);
            graph_variables(right, output);
            if let Some(expression) = expression {
                expression_variables(expression, output);
            }
        }
        GraphPattern::Filter { inner, expr } => {
            graph_variables(inner, output);
            expression_variables(expr, output);
        }
        GraphPattern::Extend {
            inner, variable, ..
        } => {
            graph_variables(inner, output);
            output.insert(variable.clone());
        }
        GraphPattern::Project { inner, variables } => {
            graph_variables(inner, output);
            output.extend(variables.iter().cloned());
        }
        GraphPattern::Graph { inner, name } | GraphPattern::Service { inner, name, .. } => {
            graph_variables(inner, output);
            if let spargebra::term::NamedNodePattern::Variable(variable) = name {
                output.insert(variable.clone());
            }
        }
        GraphPattern::Distinct { inner }
        | GraphPattern::Reduced { inner }
        | GraphPattern::Slice { inner, .. } => graph_variables(inner, output),
        GraphPattern::OrderBy { inner, expression } => {
            graph_variables(inner, output);
            for order in expression {
                let (OrderExpression::Asc(expression) | OrderExpression::Desc(expression)) = order;
                expression_variables(expression, output);
            }
        }
        GraphPattern::Group {
            inner,
            variables,
            aggregates,
        } => {
            graph_variables(inner, output);
            output.extend(variables.iter().cloned());
            for (variable, aggregate) in aggregates {
                output.insert(variable.clone());
                if let AggregateExpression::FunctionCall { expr, .. } = aggregate {
                    expression_variables(expr, output);
                }
            }
        }
    }
}

fn expression_variables(expression: &Expression, output: &mut Variables) {
    match expression {
        Expression::Variable(variable) | Expression::Bound(variable) => {
            output.insert(variable.clone());
        }
        Expression::NamedNode(_) | Expression::Literal(_) => {}
        // RDFLib's RelationalExpression deliberately publishes an empty `_vars`.
        Expression::Equal(..)
        | Expression::Greater(..)
        | Expression::GreaterOrEqual(..)
        | Expression::Less(..)
        | Expression::LessOrEqual(..)
        | Expression::In(..) => {}
        Expression::Exists(inner) => graph_variables(inner, output),
        Expression::Or(a, b)
        | Expression::And(a, b)
        | Expression::SameTerm(a, b)
        | Expression::Add(a, b)
        | Expression::Subtract(a, b)
        | Expression::Multiply(a, b)
        | Expression::Divide(a, b) => {
            expression_variables(a, output);
            expression_variables(b, output);
        }
        Expression::UnaryPlus(inner) | Expression::UnaryMinus(inner) | Expression::Not(inner) => {
            expression_variables(inner, output)
        }
        Expression::If(a, b, c) => {
            expression_variables(a, output);
            expression_variables(b, output);
            expression_variables(c, output);
        }
        Expression::Coalesce(args) | Expression::FunctionCall(_, args) => {
            for argument in args {
                expression_variables(argument, output);
            }
        }
    }
}
