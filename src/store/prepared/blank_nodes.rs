//! Query blanks are binding keys; constants and template blanks are not.
use std::collections::{HashMap, HashSet};

use super::expression::InputExpression;
use super::pattern::{Pattern, ScopeKey, SourceVariables};
use super::term::Atom;
use crate::{Error, Result};
use oxigraph::model::Term;

pub(super) struct QueryBlanks {
    // bounded: one prepared query's names and blank labels, never RDF input data.
    used: HashSet<String>,
    blanks: HashMap<String, String>,
    next: usize,
}

impl QueryBlanks {
    pub(super) fn new(used: HashSet<String>) -> Self {
        Self {
            used,
            blanks: HashMap::new(),
            next: 0,
        }
    }

    pub(super) fn into_used(self) -> HashSet<String> {
        self.used
    }

    fn fresh(&mut self) -> String {
        loop {
            let name = format!("__prepared_blank_{}", self.next);
            self.next += 1;
            if self.used.insert(name.clone()) {
                return name;
            }
        }
    }

    fn blank(&mut self, label: String) -> String {
        if let Some(name) = self.blanks.get(&label) {
            return name.clone();
        }
        let name = self.fresh();
        self.blanks.insert(label, name.clone());
        name
    }

    fn scope(&mut self, variables: &mut SourceVariables) -> Result<()> {
        if let SourceVariables::Present { keys } = variables {
            for key in keys {
                let owned = std::mem::replace(
                    key,
                    ScopeKey::Variable {
                        name: String::new(),
                    },
                );
                let name = match owned {
                    ScopeKey::Variable { name } => name,
                    ScopeKey::Blank { label } => self.blank(label),
                    // These cannot equal an RDFLib Variable/BNode binding key.
                    // Preserve their nonmatching membership without aliasing a
                    // user variable or an initial parameter of the same spelling.
                    ScopeKey::String { value } => {
                        drop(value);
                        self.fresh()
                    }
                    ScopeKey::Constant { term } => {
                        if matches!(term.term()?, Term::BlankNode(_)) {
                            return Err(Error::Invalid(
                                "Blank scope keys require the blank kind".into(),
                            ));
                        }
                        self.fresh()
                    }
                };
                *key = ScopeKey::Variable { name };
            }
        }
        Ok(())
    }

    fn expression(&mut self, value: &mut InputExpression) -> Result<()> {
        match value {
            InputExpression::Atom { .. } => (), // Expression blanks are constants.
            InputExpression::Exists { pattern } => self.pattern(pattern)?,
            InputExpression::Unary { inner, .. } => self.expression(inner)?,
            InputExpression::Binary { left, right, .. } => {
                self.expression(left)?;
                self.expression(right)?;
            }
        }
        Ok(())
    }

    pub(super) fn pattern(&mut self, pattern: &mut Pattern) -> Result<()> {
        match pattern {
            Pattern::Bgp { triples } => {
                for triple in triples {
                    for value in triple {
                        if let Atom::Term { term } = value
                            && let Some(label) = term.blank_node_label()
                        {
                            *value = Atom::Variable {
                                variable: self.blank(label.to_owned()),
                            };
                        }
                    }
                }
            }
            Pattern::Join { left, right, .. }
            | Pattern::Union { left, right }
            | Pattern::Minus { left, right } => {
                self.pattern(left)?;
                self.pattern(right)?;
            }
            Pattern::Optional {
                left,
                right,
                expression,
                variables,
            } => {
                self.pattern(left)?;
                self.pattern(right)?;
                self.expression(expression)?;
                self.scope(variables)?;
            }
            Pattern::Filter {
                inner,
                expression,
                variables,
                ..
            } => {
                self.pattern(inner)?;
                self.expression(expression)?;
                self.scope(variables)?;
            }
            Pattern::Extend {
                inner, expression, ..
            } => {
                self.pattern(inner)?;
                self.expression(expression)?;
            }
            Pattern::Orderby { inner, expressions } => {
                self.pattern(inner)?;
                for order in expressions {
                    self.expression(&mut order.expression)?;
                }
            }
            Pattern::Project { inner, .. }
            | Pattern::Distinct { inner }
            | Pattern::Reduced { inner }
            | Pattern::Slice { inner, .. }
            | Pattern::Graph { inner, .. } => self.pattern(inner)?,
            Pattern::Values { .. } => (), // VALUES blanks are constants, not BGP variables.
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_transport_cannot_disguise_a_binding_blank_as_a_constant() {
        let mut variables: SourceVariables = serde_json::from_str(r#"{"state":"present","keys":[{"kind":"constant","term":{"type":"bnode","value":"b"}}]}"#).unwrap();
        assert!(
            QueryBlanks::new(HashSet::new())
                .scope(&mut variables)
                .is_err()
        );
    }

    #[test]
    fn only_bgp_positions_become_query_variables() {
        let mut pattern: Pattern = serde_json::from_str(r#"{
          "kind":"graph","name":{"term":{"type":"bnode","value":"b"}},
          "inner":{"kind":"filter","variables":{"state":"absent"},"no_isolated_scope":false,
            "expression":{"kind":"atom","value":{"term":{"type":"bnode","value":"b"}}},
            "inner":{"kind":"bgp","triples":[[{"term":{"type":"bnode","value":"b"}},{"term":{"type":"uri","value":"urn:p"}},{"term":{"type":"literal","value":"A"}}]]}}
        }"#).unwrap();
        QueryBlanks::new(HashSet::new())
            .pattern(&mut pattern)
            .unwrap();
        let Pattern::Graph {
            inner,
            name: Atom::Term { term },
        } = pattern
        else {
            panic!("typed graph constant");
        };
        assert_eq!(term.blank_node_label(), Some("b"));
        let Pattern::Filter {
            inner,
            expression:
                InputExpression::Atom {
                    value: Atom::Term { term },
                },
            ..
        } = *inner
        else {
            panic!("typed expression constant");
        };
        assert_eq!(term.blank_node_label(), Some("b"));
        let Pattern::Bgp { triples } = *inner else {
            panic!("BGP");
        };
        assert!(matches!(triples[0][0], Atom::Variable { .. }));
    }
}
