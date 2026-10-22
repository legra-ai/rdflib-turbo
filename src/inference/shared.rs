//! Lower portable rule definitions into backend-owned compiled queries.

use datalog_structural_model::{Atom, Literal, Rule, Term};
use oxigraph::model::NamedNodeRef;
use oxigraph::sparql::{PreparedSparqlQuery, SparqlEvaluator};
use rdf_reasoning::{RuleId::*, TRIPLE_RELATION};
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
            let Literal::Positive(atom) = literal else {
                return Err(Error::Unsupported(
                    "Shared inference adapter requires positive triple atoms".into(),
                ));
            };
            body.push_str(&Self::atom(atom)?);
        }
        SparqlEvaluator::new()
            .parse_query(&format!("CONSTRUCT {{ {head} }} WHERE {{ {body} }}"))
            .map(Self)
            .map_err(|error| Error::Runtime(format!("Invalid shared inference rule: {error}")))
    }

    fn atom(atom: &Atom) -> Result<String> {
        if atom.predicate.as_str() != TRIPLE_RELATION || atom.terms.len() != 3 {
            return Err(Error::Unsupported(
                "Shared inference adapter requires the three-place RDF triple relation".into(),
            ));
        }
        let mut pattern = String::new();
        for term in &atom.terms {
            let value = match term {
                Term::Variable(variable) => Variable::new(variable.as_str())
                    .map_err(|error| Error::Unsupported(format!("Invalid rule variable: {error}")))?
                    .to_string(),
                Term::Constant(constant) => NamedNodeRef::new(constant.as_str())
                    .map_err(|error| Error::Unsupported(format!("Invalid rule IRI: {error}")))?
                    .to_string(),
                Term::TripleTerm(_) | Term::Skolem { .. } => {
                    return Err(Error::Unsupported(
                        "Shared inference adapter requires flat terms".into(),
                    ));
                }
            };
            pattern.push_str(&value);
            pattern.push(' ');
        }
        pattern.push_str(". ");
        Ok(pattern)
    }

    pub(super) fn into_query(self) -> PreparedSparqlQuery {
        self.0
    }
}

impl CompiledRule {
    pub(super) fn for_mode(mode: &str) -> Result<Vec<PreparedSparqlQuery>> {
        // bounded: twelve RDFS clauses plus four OWL property clauses.
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
            selected.extend([PrpSymp, PrpTrp, PrpInv1, PrpInv2]);
        }
        selected
            .into_iter()
            .map(|id| CompiledRule::try_new(&id.definition()).map(CompiledRule::into_query))
            .collect()
    }
}
