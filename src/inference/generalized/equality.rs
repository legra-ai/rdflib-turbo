//! Lower shared equality clauses only for derivations outside the RDF kernel.

use oxigraph::sparql::PreparedSparqlQuery;
use rdf_reasoning::RuleId::{self, *};

use super::{Generalized, query};
use crate::Result;
use crate::inference::Pending;
use crate::inference::backend::Graph;
use crate::inference::shared::{CompiledRule, RulePartition};

pub(in crate::inference) struct EqualityRules {
    seeds: Vec<PreparedSparqlQuery>,
    generalized: Vec<PreparedSparqlQuery>,
}

impl EqualityRules {
    pub(in crate::inference) const RULES: [RuleId; 8] = [
        EqRefSubject,
        EqRefPredicate,
        EqRefObject,
        EqSym,
        EqTrans,
        EqRepS,
        EqRepP,
        EqRepO,
    ];

    pub(in crate::inference) fn try_new() -> Result<Self> {
        // bounded: only these four equality clauses can produce a non-RDF head
        // from RDF-only premises. The other heads keep RDF subject/predicate slots.
        let seeds = [EqRefObject, EqSym, EqRepS, EqRepP]
            .into_iter()
            .map(|id| {
                CompiledRule::partition(&id.definition(), RulePartition::GeneralizedHead)
                    .map(CompiledRule::into_query)
            })
            .collect::<Result<Vec<_>>>()?;
        // bounded: twelve positive-premise positions across eight shared clauses.
        let mut generalized = Vec::new();
        for id in Self::RULES {
            let rule = id.definition();
            for index in 0..rule.body.len() {
                generalized.push(
                    CompiledRule::partition(&rule, RulePartition::GeneralizedPremise(index))?
                        .into_query(),
                );
            }
        }
        Ok(Self { seeds, generalized })
    }

    pub(in crate::inference) fn apply(
        &self,
        current: &Generalized,
        data: &impl Graph,
        ordinary: &mut Pending,
        pending: &mut Generalized,
    ) -> Result<()> {
        for query in &self.seeds {
            current.retain(data.query(query.clone())?, data, ordinary, pending)?;
        }
        if current.rows.is_empty() {
            return Ok(());
        }
        for query in &self.generalized {
            let results = query
                .clone()
                .on_queryable_dataset(query::View {
                    data,
                    extra: current,
                })
                .execute()?;
            current.retain(results, data, ordinary, pending)?;
        }
        Ok(())
    }
}
