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
        self.apply_inner(
            data,
            pending,
            #[cfg(all(test, feature = "allocation-profiling"))]
            0,
        )
    }

    #[cfg(all(test, feature = "allocation-profiling"))]
    pub(super) fn apply_traced(
        &self,
        data: &impl Graph,
        pending: &mut Pending,
        round: usize,
    ) -> Result<()> {
        self.apply_inner(data, pending, round)
    }

    fn apply_inner(
        &self,
        data: &impl Graph,
        pending: &mut Pending,
        #[cfg(all(test, feature = "allocation-profiling"))] round: usize,
    ) -> Result<()> {
        for (ordinal, clause) in self.clauses.iter().enumerate() {
            #[cfg(not(all(test, feature = "allocation-profiling")))]
            let _ = ordinal;
            if self.bootstrap || clause.wildcard || clause.reads & self.dirty != 0 {
                #[cfg(all(test, feature = "allocation-profiling"))]
                crate::allocation_tests::trace::mark_clause(
                    "clause_before",
                    round,
                    ordinal,
                    pending.trace_counts(),
                );
                apply_rules(data, std::slice::from_ref(&clause.query), pending, false)?;
                #[cfg(all(test, feature = "allocation-profiling"))]
                crate::allocation_tests::trace::mark_clause(
                    "clause_after",
                    round,
                    ordinal,
                    pending.trace_counts(),
                );
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

#[cfg(all(test, feature = "allocation-profiling"))]
#[path = "scheduling_trace_tests.rs"]
mod trace_tests;
