//! Lower portable rule definitions into backend-owned compiled queries.

use datalog_structural_model::{Atom, BuiltinFilter, Literal, Rule, Term};
use oxigraph::model::{NamedNode, NamedNodeRef};
use oxigraph::sparql::{PreparedSparqlQuery, SparqlEvaluator};
use rdf_reasoning::{
    LITERAL_VALUE_RELATION, ListRuleId, Owlrl762RuleId, RDF_LIST_MEMBER_RELATION, RuleId::*,
    TRIPLE_RELATION,
};
use spargebra::term::Variable;

use crate::{Error, Result};

pub(super) struct CompiledRule(PreparedSparqlQuery, ReadPredicates);

pub(super) struct ReadPredicates {
    pub constants: Vec<NamedNode>,
    pub wildcard: bool,
}

impl ReadPredicates {
    fn try_new(rule: &Rule) -> Result<Self> {
        // bounded: metadata for one fixed catalogue clause, never input graph predicates.
        let mut constants = Vec::new();
        let mut wildcard = false;
        for literal in &rule.body {
            let Literal::Positive(atom) = literal else {
                continue;
            };
            if atom.predicate.as_str() == RDF_LIST_MEMBER_RELATION {
                for iri in [
                    "http://www.w3.org/1999/02/22-rdf-syntax-ns#first",
                    "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest",
                ] {
                    constants.push(NamedNode::new_unchecked(iri));
                }
            } else {
                // The query compiler has already validated the triple relation/arity.
                match &atom.terms[1] {
                    Term::Constant(value) => {
                        constants.push(NamedNode::new(value.as_str()).map_err(|error| {
                            Error::Unsupported(format!("Invalid rule predicate: {error}"))
                        })?)
                    }
                    Term::Variable(_) => wildcard = true,
                    _ => unreachable!("query compiler rejected non-flat rule terms"),
                }
            }
        }
        Ok(Self {
            constants,
            wildcard,
        })
    }
}

impl CompiledRule {
    #[cfg(test)]
    pub(super) fn from_catalogue(id: rdf_reasoning::RuleId) -> Result<Self> {
        Self::try_new(&Self::catalogue_definition(id)?)
    }

    fn catalogue_definition(id: rdf_reasoning::RuleId) -> Result<Rule> {
        let mut rule = id.definition();
        if matches!(id, CaxEqc1 | CaxEqc2) {
            rule = Self::without_self_membership(id, &rule)?;
        }
        Ok(rule)
    }

    fn without_self_membership(id: rdf_reasoning::RuleId, rule: &Rule) -> Result<Rule> {
        let invalid = || Error::Unsupported("Unexpected equivalent-class catalogue shape".into());
        let [
            Literal::Positive(equivalence),
            Literal::Positive(membership),
        ] = rule.body.as_slice()
        else {
            return Err(invalid());
        };
        for atom in [&rule.head, equivalence, membership] {
            if atom.predicate.as_str() != TRIPLE_RELATION || atom.terms.len() != 3 {
                return Err(invalid());
            }
        }
        let is_constant =
            |term: &Term, iri: &str| matches!(term, Term::Constant(value) if value.as_str() == iri);
        if !is_constant(
            &membership.terms[1],
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
        ) || rule.head.terms[1] != membership.terms[1]
            || rule.head.terms[0] != membership.terms[0]
            || !is_constant(
                &equivalence.terms[1],
                "http://www.w3.org/2002/07/owl#equivalentClass",
            )
        {
            return Err(invalid());
        }
        let source = &membership.terms[2];
        let target = &rule.head.terms[2];
        let expected = match id {
            CaxEqc1 => [&equivalence.terms[0], &equivalence.terms[2]],
            CaxEqc2 => [&equivalence.terms[2], &equivalence.terms[0]],
            _ => return Err(invalid()),
        };
        if expected != [source, target]
            || source == target
            || source == &rule.head.terms[0]
            || target == &rule.head.terms[0]
            || [&rule.head.terms[0], source, target]
                .iter()
                .any(|term| !matches!(term, Term::Variable(_)))
        {
            return Err(invalid());
        }
        // With identical classes the head is the membership body fact, already
        // visible to Pending::push in the source even for a separate destination.
        let mut optimized = rule.clone();
        optimized.body.push(Literal::Filter(BuiltinFilter::Ne(
            source.clone(),
            target.clone(),
        )));
        Ok(optimized)
    }

    pub(super) fn try_new(rule: &Rule) -> Result<Self> {
        let (head, body) = Self::validated_parts(rule, false)?;
        Self::prepare(&head, &body, ReadPredicates::try_new(rule)?)
    }

    pub(super) fn with_literal_values(rule: &Rule, evaluator: SparqlEvaluator) -> Result<Self> {
        let (head, body) = Self::validated_parts(rule, true)?;
        let query = evaluator
            .parse_query(&format!("CONSTRUCT {{ {head} }} WHERE {{ {body} }}"))
            .map_err(|error| Error::Runtime(format!("Invalid shared inference rule: {error}")))?;
        // The dedicated literal substitution query is run before scheduling.
        // bounded: no graph-derived predicate metadata is retained.
        Ok(Self(
            query,
            ReadPredicates {
                constants: Vec::new(),
                wildcard: true,
            },
        ))
    }

    fn validated_parts(rule: &Rule, literal_values: bool) -> Result<(String, String)> {
        if !rule.is_safe() {
            return Err(Error::Unsupported("Unsafe shared inference rule".into()));
        }
        let head = Self::atom(&rule.head)?;
        let mut body = String::new();
        for literal in &rule.body {
            match literal {
                Literal::Positive(atom) => body.push_str(&Self::body_atom(atom, literal_values)?),
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
        Ok((head, body))
    }

    fn prepare(head: &str, body: &str, reads: ReadPredicates) -> Result<Self> {
        let query = SparqlEvaluator::new()
            .parse_query(&format!("CONSTRUCT {{ {head} }} WHERE {{ {body} }}"))
            .map_err(|error| Error::Runtime(format!("Invalid shared inference rule: {error}")))?;
        Ok(Self(query, reads))
    }

    fn compiled_catalogue(rules: &[Rule]) -> Result<Vec<Self>> {
        if let [rule] = rules {
            return Ok(vec![Self::try_new(rule)?]);
        }
        // bounded: only the fixed published catalogue, never caller graph facts.
        // Validate every original rule before grouping can hide an invalid head.
        let lowered = rules
            .iter()
            .map(|rule| Self::validated_parts(rule, false).map(|(head, body)| (rule, head, body)))
            .collect::<Result<Vec<_>>>()?;
        // Exact ordered AST equality retains variable names and filter order.
        // Dependencies belong to the common body, independent of its heads.
        let mut groups: Vec<(&Rule, String, String)> = Vec::new();
        for (rule, head, body) in lowered {
            if let Some(group) = groups.iter_mut().find(|group| group.0.body == rule.body) {
                group.1.push_str(&head);
            } else {
                groups.push((rule, head, body));
            }
        }
        groups
            .into_iter()
            .map(|(rule, head, body)| Self::prepare(&head, &body, ReadPredicates::try_new(rule)?))
            .collect()
    }

    #[cfg(test)]
    pub(super) fn try_catalogue(rules: &[Rule]) -> Result<Vec<PreparedSparqlQuery>> {
        Self::compiled_catalogue(rules)
            .map(|rules| rules.into_iter().map(Self::into_query).collect())
    }

    fn body_atom(atom: &Atom, literal_values: bool) -> Result<String> {
        if atom.predicate.as_str() == LITERAL_VALUE_RELATION {
            if !literal_values || atom.terms.len() != 2 {
                return Err(Error::Unsupported("Literal value classes require an explicit value-key evaluator and two operands".into()));
            }
            let literal = Self::term(&atom.terms[0])?;
            let key = Self::term(&atom.terms[1])?;
            // Only these two variables escape the subquery. Private scan names
            // cannot collide with either operand or an outer rule variable.
            let mut stem = "__literal".to_owned();
            while [literal.as_str(), key.as_str()]
                .iter()
                .any(|term| term.starts_with(&format!("?{stem}")))
            {
                stem.push('_');
            }
            return Ok(format!(
                "{{ SELECT DISTINCT {literal} {key} WHERE {{ ?{stem}s ?{stem}p {literal} . BIND (<{LITERAL_VALUE_RELATION}>({literal}) AS {key}) FILTER (BOUND({key})) }} }} "
            ));
        }
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

    pub(super) fn into_parts(self) -> (PreparedSparqlQuery, ReadPredicates) {
        (self.0, self.1)
    }
}

impl CompiledRule {
    #[cfg(test)]
    pub(super) fn for_mode(mode: &str) -> Result<Vec<PreparedSparqlQuery>> {
        Self::compiled_for_mode(mode).map(|rules| rules.into_iter().map(Self::into_query).collect())
    }

    pub(super) fn compiled_for_mode(mode: &str) -> Result<Vec<Self>> {
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
        // bounded: the fixed published catalogue; no graph facts are retained.
        let mut rules = selected
            .into_iter()
            .map(Self::catalogue_definition)
            .collect::<Result<Vec<_>>>()?;
        if mode != "rdfs" {
            for id in Owlrl762RuleId::ALL {
                rules.push(id.definition());
            }
            for id in ListRuleId::ALL {
                // The native property path supplies the exact auxiliary relation;
                // it does not materialize membership facts in the caller's graph.
                if matches!(id, ListRuleId::MemberFirst | ListRuleId::MemberRest) {
                    continue;
                }
                rules.push(id.definition());
            }
        }
        Self::compiled_catalogue(&rules)
    }
}

#[cfg(test)]
#[path = "shared_cax_tests.rs"]
mod cax_tests;
