//! Fresh native names are checked against the complete prepared-query scope.
use std::collections::HashSet;

use super::expression::InputExpression;
use super::pattern::{Pattern, ScopeKey, SourceVariables};
use super::term::Atom;

pub(super) fn variables(
    pattern: &Pattern,
    describe: &[Atom],
    template: &[[Atom; 3]],
) -> HashSet<String> {
    // bounded: names from this one query AST, never a relation in the RDF input.
    let mut used = HashSet::new();
    pattern_variables(pattern, &mut used);
    for value in describe {
        atom(value, &mut used);
    }
    for triple in template {
        for value in triple {
            atom(value, &mut used);
        }
    }
    used
}

fn atom(value: &Atom, used: &mut HashSet<String>) {
    if let Atom::Variable { variable } = value {
        used.insert(variable.clone());
    }
}

fn expression(value: &InputExpression, used: &mut HashSet<String>) {
    match value {
        InputExpression::Atom { value } => atom(value, used),
        InputExpression::Exists { pattern } => pattern_variables(pattern, used),
        InputExpression::Unary { inner, .. } => expression(inner, used),
        InputExpression::Binary { left, right, .. } => {
            expression(left, used);
            expression(right, used);
        }
    }
}

fn pattern_variables(pattern: &Pattern, used: &mut HashSet<String>) {
    match pattern {
        Pattern::Bgp { triples } => {
            for triple in triples {
                for value in triple {
                    atom(value, used);
                }
            }
        }
        Pattern::Join { left, right, .. }
        | Pattern::Union { left, right }
        | Pattern::Minus { left, right } => {
            pattern_variables(left, used);
            pattern_variables(right, used);
        }
        Pattern::Optional {
            left,
            right,
            expression: guard,
            variables,
        } => {
            pattern_variables(left, used);
            pattern_variables(right, used);
            expression(guard, used);
            if let SourceVariables::Present { keys } = variables {
                for key in keys {
                    if let ScopeKey::Variable { name } = key {
                        used.insert(name.clone());
                    }
                }
            }
        }
        Pattern::Project { inner, variables } => {
            pattern_variables(inner, used);
            used.extend(variables.iter().cloned());
        }
        Pattern::Extend {
            inner,
            variable,
            expression: value,
        } => {
            pattern_variables(inner, used);
            used.insert(variable.clone());
            expression(value, used);
        }
        Pattern::Filter {
            inner,
            expression: value,
            variables,
            ..
        } => {
            pattern_variables(inner, used);
            expression(value, used);
            if let SourceVariables::Present { keys } = variables {
                for key in keys {
                    if let ScopeKey::Variable { name } = key {
                        used.insert(name.clone());
                    }
                }
            }
        }
        Pattern::Values { variables, rows } => {
            used.extend(variables.iter().cloned());
            for row in rows {
                for value in row.iter().flatten() {
                    atom(value, used);
                }
            }
        }
        Pattern::Orderby { inner, expressions } => {
            pattern_variables(inner, used);
            for order in expressions {
                expression(&order.expression, used);
            }
        }
        Pattern::Distinct { inner } | Pattern::Reduced { inner } | Pattern::Slice { inner, .. } => {
            pattern_variables(inner, used)
        }
        Pattern::Graph { inner, name } => {
            pattern_variables(inner, used);
            atom(name, used);
        }
    }
}
