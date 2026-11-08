//! Small checked bindings for the four portable numeric clauses.
use crate::{Error, Result};
use datalog_structural_model::{Atom, Term as Pattern};
use oxigraph::model::{NamedNode, Term};
use rdf_reasoning::OwlNumericValue;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Cell {
    Rdf(Term),
    Value(OwlNumericValue),
}

#[derive(Clone, Default)]
pub(super) struct Bindings(BTreeMap<String, Cell>);

impl Bindings {
    pub(super) fn resolve(&self, term: &Pattern) -> Result<Cell> {
        match term {
            Pattern::Variable(variable) => self
                .0
                .get(variable.as_str())
                .cloned()
                .ok_or_else(|| Error::Unsupported("Unbound numeric clause operand".into())),
            Pattern::Constant(value) => Ok(Cell::Rdf(
                NamedNode::new(value.as_str())
                    .map_err(|error| {
                        Error::Unsupported(format!("Invalid numeric clause IRI: {error}"))
                    })?
                    .into(),
            )),
            _ => Err(Error::Unsupported(
                "Numeric clauses require flat terms".into(),
            )),
        }
    }

    pub(super) fn bound(&self, term: &Pattern) -> Result<Option<Cell>> {
        match term {
            Pattern::Variable(variable) => Ok(self.0.get(variable.as_str()).cloned()),
            _ => self.resolve(term).map(Some),
        }
    }

    pub(super) fn join(&self, atom: &Atom, row: &[Cell]) -> Result<Option<Self>> {
        if atom.terms.len() != row.len() {
            return Err(Error::Unsupported(
                "Numeric clause relation arity mismatch".into(),
            ));
        }
        // bounded: at most four variables in the fixed portable numeric catalogue.
        let mut next = self.clone();
        for (term, cell) in atom.terms.iter().zip(row) {
            if let Some(previous) = next.bound(term)? {
                if previous != *cell {
                    return Ok(None);
                }
            } else if let Pattern::Variable(variable) = term {
                if next.0.len() >= 4 {
                    return Err(Error::Unsupported(
                        "Numeric clause binding limit exceeded".into(),
                    ));
                }
                next.0.insert(variable.as_str().to_owned(), cell.clone());
            }
        }
        Ok(Some(next))
    }

    pub(super) fn rdf(&self, term: &Pattern) -> Result<Term> {
        match self.resolve(term)? {
            Cell::Rdf(term) => Ok(term),
            Cell::Value(_) => Err(Error::Unsupported(
                "Numeric value keys cannot escape as RDF terms".into(),
            )),
        }
    }
}
