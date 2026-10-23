//! Lower portable rule definitions into backend-owned compiled queries.

use datalog_structural_model::{Atom, BuiltinFilter, Literal, Rule, Term};
use oxigraph::model::NamedNodeRef;
use oxigraph::sparql::{PreparedSparqlQuery, SparqlEvaluator};
use rdf_reasoning::{
    ListRuleId, Owlrl762RuleId, RDF_LIST_MEMBER_RELATION, RuleId::*, TRIPLE_RELATION,
};
use spargebra::term::Variable;

use crate::{Error, Result};

pub(super) struct CompiledRule(PreparedSparqlQuery);

impl CompiledRule {
    pub(super) fn try_new(rule: &Rule) -> Result<Self> {
        if !rule.is_safe() {
            return Err(Error::Unsupported("Unsafe shared inference rule".into()));
        }
        let head = Self::atom(&rule.head)?;
        let mut body = String::new();
        for literal in &rule.body {
            match literal {
                Literal::Positive(atom) => body.push_str(&Self::body_atom(atom)?),
                Literal::Filter(BuiltinFilter::Ne(left, right)) => {
                    body.push_str(&format!(
                        "FILTER(!sameTerm({}, {})) ",
                        Self::term(left)?,
                        Self::term(right)?
                    ));
                }
                _ => {
                    return Err(Error::Unsupported(
                        "Shared inference adapter requires positive triples or exact inequality"
                            .into(),
                    ));
                }
            }
        }
        SparqlEvaluator::new()
            .parse_query(&format!("CONSTRUCT {{ {head} }} WHERE {{ {body} }}"))
            .map(Self)
            .map_err(|error| Error::Runtime(format!("Invalid shared inference rule: {error}")))
    }

    fn body_atom(atom: &Atom) -> Result<String> {
        if atom.predicate.as_str() != RDF_LIST_MEMBER_RELATION {
            return Self::atom(atom);
        }
        if atom.terms.len() != 2 {
            return Err(Error::Unsupported(
                "Shared list membership requires exactly two terms".into(),
            ));
        }
        Ok(format!(
            "{} <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest>*/<http://www.w3.org/1999/02/22-rdf-syntax-ns#first> {} . ",
            Self::term(&atom.terms[0])?,
            Self::term(&atom.terms[1])?
        ))
    }

    fn atom(atom: &Atom) -> Result<String> {
        if atom.predicate.as_str() != TRIPLE_RELATION || atom.terms.len() != 3 {
            return Err(Error::Unsupported(
                "Shared inference adapter requires the three-place RDF triple relation".into(),
            ));
        }
        let mut pattern = String::new();
        for term in &atom.terms {
            let value = Self::term(term)?;
            pattern.push_str(&value);
            pattern.push(' ');
        }
        pattern.push_str(". ");
        Ok(pattern)
    }

    fn term(term: &Term) -> Result<String> {
        match term {
            Term::Variable(variable) => Variable::new(variable.as_str())
                .map(|variable| variable.to_string())
                .map_err(|error| Error::Unsupported(format!("Invalid rule variable: {error}"))),
            Term::Constant(constant) => NamedNodeRef::new(constant.as_str())
                .map(|node| node.to_string())
                .map_err(|error| Error::Unsupported(format!("Invalid rule IRI: {error}"))),
            Term::TripleTerm(_) | Term::Skolem { .. } => Err(Error::Unsupported(
                "Shared inference adapter requires flat terms".into(),
            )),
        }
    }

    pub(super) fn into_query(self) -> PreparedSparqlQuery {
        self.0
    }
}

impl CompiledRule {
    pub(super) fn for_mode(mode: &str) -> Result<Vec<PreparedSparqlQuery>> {
        // bounded: the fixed published RDFS and OWL relational clause inventory.
        let mut selected = Vec::new();
        if mode != "owlrl" {
            // Resource seeding stays in the reference initialization boundary.
            selected.extend([
                Rdf1, Rdfs2, Rdfs3, Rdfs5, Rdfs6, Rdfs7, Rdfs8, Rdfs9, Rdfs10, Rdfs11, Rdfs12,
                Rdfs13,
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
            .map(|id| Self::try_new(&id.definition()).map(Self::into_query))
            .collect::<Result<Vec<_>>>()?;
        if mode != "rdfs" {
            for id in Owlrl762RuleId::ALL {
                queries.push(Self::try_new(&id.definition())?.into_query());
            }
            for id in ListRuleId::ALL {
                // The native property path supplies the exact auxiliary relation;
                // it does not materialize membership facts in the caller's graph.
                if matches!(id, ListRuleId::MemberFirst | ListRuleId::MemberRest) {
                    continue;
                }
                queries.push(Self::try_new(&id.definition())?.into_query());
            }
        }
        Ok(queries)
    }
}
