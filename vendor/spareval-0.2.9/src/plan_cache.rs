//! Bounded reuse of data-independent algebra with initial bindings in the key.

use oxrdf::Variable;
use spargebra::algebra::GraphPattern as ParsedPattern;
use sparopt::{Optimizer, algebra::GraphPattern};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};

const CAPACITY: usize = 128;

struct PlanCache {
    capacity: usize,
    entries: VecDeque<(ParsedPattern, Vec<Variable>, Arc<GraphPattern>)>,
}

pub(super) fn prepare(pattern: &ParsedPattern, variables: Vec<Variable>) -> Arc<GraphPattern> {
    static CACHE: OnceLock<Mutex<PlanCache>> = OnceLock::new();
    CACHE
        .get_or_init(|| Mutex::new(PlanCache::new(CAPACITY)))
        .lock()
        .expect("SPARQL plan cache poisoned")
        .prepare(pattern, variables)
}

impl PlanCache {
    fn new(capacity: usize) -> Self {
        assert!(
            capacity > 0,
            "SPARQL plan cache must have positive capacity"
        );
        Self {
            capacity,
            entries: VecDeque::new(),
        }
    }

    fn prepare(
        &mut self,
        pattern: &ParsedPattern,
        mut variables: Vec<Variable>,
    ) -> Arc<GraphPattern> {
        variables.sort_unstable();
        variables.dedup();
        if let Some(index) = self
            .entries
            .iter()
            .position(|(parsed, bound, _)| parsed == pattern && bound == &variables)
        {
            let entry = self.entries.remove(index).expect("cached plan exists");
            let result = entry.2.clone();
            self.entries.push_back(entry);
            return result;
        }
        let plan = Arc::new(Optimizer::optimize_graph_pattern_with_bound_variables(
            GraphPattern::from(pattern),
            variables.iter().cloned(),
        ));
        if self.entries.len() == self.capacity {
            self.entries.pop_front();
        }
        self.entries
            .push_back((pattern.clone(), variables, plan.clone()));
        plan
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spargebra::{Query, SparqlParser};

    fn pattern(text: &str) -> ParsedPattern {
        let Query::Select { pattern, .. } = SparqlParser::new().parse_query(text).unwrap() else {
            panic!("expected SELECT")
        };
        pattern
    }

    #[test]
    fn reuses_algebra_but_distinguishes_initial_bindings_and_query_content() {
        let mut cache = PlanCache::new(2);
        let pattern = pattern("SELECT ?this ?x WHERE { FILTER(BOUND(?this)) BIND(?this AS ?x) }");
        let this = Variable::new("this").unwrap();
        let x = Variable::new("x").unwrap();
        let first = cache.prepare(&pattern, vec![this.clone(), x.clone()]);
        let repeated = cache.prepare(&pattern, vec![x, this.clone()]);
        assert!(Arc::ptr_eq(&first, &repeated));
        let unbound = cache.prepare(&pattern, vec![]);
        assert!(!Arc::ptr_eq(&first, &unbound));
        assert_ne!(first, unbound);
        assert!(Arc::ptr_eq(&unbound, &cache.prepare(&pattern, vec![])));
    }

    #[test]
    fn cache_is_bounded_and_evicts_least_recently_used_plans() {
        let mut cache = PlanCache::new(2);
        let a = pattern("SELECT ?s WHERE { ?s <urn:a> ?o }");
        let b = pattern("SELECT ?s WHERE { ?s <urn:b> ?o }");
        let c = pattern("SELECT ?s WHERE { ?s <urn:c> ?o }");
        let first = cache.prepare(&a, vec![]);
        let second = cache.prepare(&b, vec![]);
        assert!(Arc::ptr_eq(&first, &cache.prepare(&a, vec![])));
        cache.prepare(&c, vec![]);
        assert!(!Arc::ptr_eq(&second, &cache.prepare(&b, vec![])));
    }
}
