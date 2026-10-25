//! Frozen original public7008 catalogue selection and one-query-per-clause oracle.
use super::super::shared::CompiledRule;
use crate::Result;
use oxigraph::sparql::PreparedSparqlQuery;
use rdf_reasoning::{ListRuleId, Owlrl762RuleId, RuleId::*};

pub(super) fn queries(mode: &str) -> Result<Vec<PreparedSparqlQuery>> {
    // bounded: the fixed published RDFS and OWL relational clause inventory.
    let mut selected = Vec::new();
    if mode != "owlrl" {
        // Resource seeding stays in the reference initialization boundary.
        selected.extend([
            Rdf1, Rdfs2, Rdfs3, Rdfs5, Rdfs6, Rdfs7, Rdfs8, Rdfs9, Rdfs10, Rdfs11, Rdfs12, Rdfs13,
        ]);
    } else {
        selected.extend([Rdfs2, Rdfs3, Rdfs7, Rdfs9]);
    }
    if mode != "rdfs" {
        selected.extend([
            PrpSymp,
            PrpTrp,
            PrpInv1,
            PrpInv2,
            PrpFp,
            PrpIfp,
            ClsSvf1,
            ClsSvf2,
            ClsAvf,
            ClsHv1,
            ClsHv2,
            CaxEqc1,
            CaxEqc2,
            ScmClsSelfSubclass,
            ScmClsSelfEquivalent,
            ScmClsThing,
            ScmClsNothing,
            ScmEqc2,
            ScmOpSelfSubproperty,
            ScmOpSelfEquivalent,
            ScmDpSelfSubproperty,
            ScmDpSelfEquivalent,
            ScmDom1,
            ScmDom2,
            ScmRng1,
            ScmRng2,
            ScmHv,
            ScmSvf1,
            ScmSvf2,
            ScmAvf1,
            ScmAvf2,
        ]);
        if mode == "owlrl" {
            selected.push(Rdfs6);
        }
    }
    // bounded: one prepared query per fixed catalogue clause.
    let mut queries = selected
        .into_iter()
        .map(|id| CompiledRule::try_new(&id.definition()).map(CompiledRule::into_query))
        .collect::<Result<Vec<_>>>()?;
    if mode != "rdfs" {
        for id in Owlrl762RuleId::ALL {
            queries.push(CompiledRule::try_new(&id.definition())?.into_query());
        }
        for id in ListRuleId::ALL {
            // The native property path supplies the exact auxiliary relation;
            // it does not materialize membership facts in the caller's graph.
            if matches!(id, ListRuleId::MemberFirst | ListRuleId::MemberRest) {
                continue;
            }
            queries.push(CompiledRule::try_new(&id.definition())?.into_query());
        }
    }
    Ok(queries)
}
