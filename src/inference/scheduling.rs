//! Conservative dependency scheduling, retaining full-graph joins for active clauses.
use std::collections::HashMap;

use oxigraph::model::{NamedNode, Term};
use oxigraph::sparql::PreparedSparqlQuery;

use super::shared::CompiledRule;
use super::{Graph, Pending, apply_rules};
use crate::{Error, Result};

struct Clause {
    query: PreparedSparqlQuery,
    reads: u64,
    wildcard: bool,
}

pub(super) struct Schedule {
    clauses: Vec<Clause>,
    predicates: HashMap<NamedNode, u64>,
    dirty: u64,
    bootstrap: bool,
}

impl Schedule {
    pub(super) fn for_mode(mode: &str) -> Result<Self> {
        // bounded: only constant predicates in the fixed compiled catalogue.
        let mut predicates = HashMap::new();
        // bounded: exactly one descriptor per fixed compiled catalogue clause.
        let mut clauses = Vec::new();
        for rule in CompiledRule::compiled_for_mode(mode)? {
            let (query, dependencies) = rule.into_parts();
            let mut reads = 0;
            for predicate in dependencies.constants {
                let count = predicates.len();
                let bit = if let Some(bit) = predicates.get(&predicate) {
                    *bit
                } else {
                    let bit = 1u64.checked_shl(count as u32).ok_or_else(|| {
                        Error::Unsupported("Inference predicate schedule exceeds 64 slots".into())
                    })?;
                    predicates.insert(predicate, bit);
                    bit
                };
                reads |= bit;
            }
            clauses.push(Clause {
                query,
                reads,
                wildcard: dependencies.wildcard,
            });
        }
        Ok(Self {
            clauses,
            predicates,
            dirty: 0,
            bootstrap: true,
        })
    }

    pub(super) fn apply(&self, data: &impl Graph, pending: &mut Pending) -> Result<()> {
        for clause in &self.clauses {
            if self.bootstrap || clause.wildcard || clause.reads & self.dirty != 0 {
                apply_rules(data, std::slice::from_ref(&clause.query), pending, false)?;
            }
        }
        Ok(())
    }

    pub(super) fn changed(&self, predicate: &Term) -> u64 {
        match predicate {
            Term::NamedNode(predicate) => self.predicates.get(predicate).copied().unwrap_or(0),
            _ => 0,
        }
    }

    pub(super) fn advance(&mut self, dirty: u64) {
        self.dirty = dirty;
        self.bootstrap = false;
    }
}
