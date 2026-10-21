use oxigraph::model::{Term, Variable};
use serde::Deserialize;
use spargebra::term::{GroundTerm, NamedNodePattern, TermPattern, TriplePattern};

use super::super::pattern::InputTerm;
use crate::{Error, Result};

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum Atom {
    Variable { variable: String },
    Term { term: InputTerm },
}

impl Atom {
    pub(super) fn build(self) -> Result<TermPattern> {
        Ok(match self {
            Self::Variable { variable } => Variable::new_unchecked(variable).into(),
            Self::Term { term } => match term.term()? {
                Term::NamedNode(value) => value.into(),
                Term::BlankNode(value) => value.into(),
                Term::Literal(value) => value.into(),
            },
        })
    }

    pub(super) fn ground(self) -> Result<GroundTerm> {
        match self.build()? {
            TermPattern::NamedNode(value) => Ok(value.into()),
            TermPattern::Literal(value) => Ok(value.into()),
            _ => Err(Error::Invalid("VALUES requires an IRI or literal".into())),
        }
    }

    pub(super) fn name(self) -> Result<NamedNodePattern> {
        match self.build()? {
            TermPattern::NamedNode(value) => Ok(value.into()),
            TermPattern::Variable(value) => Ok(value.into()),
            _ => Err(Error::Invalid(
                "Prepared graph or predicate must be an IRI or variable".into(),
            )),
        }
    }
}

pub(super) fn triple([subject, predicate, object]: [Atom; 3]) -> Result<TriplePattern> {
    Ok(TriplePattern {
        subject: subject.build()?,
        predicate: predicate.name()?,
        object: object.build()?,
    })
}
