//! Restartable numeric relations; never retain the input literal domain.
use super::bindings::{Bindings, Cell};
use crate::inference::backend::Graph;
use crate::{Error, Result};
use datalog_structural_model::Atom;
use oxigraph::model::*;
use rdf_reasoning::{
    OWL_NUMERIC_MEMBER_RELATION, OWL_NUMERIC_NONMEMBER_RELATION, OWL_NUMERIC_VALUE_RELATION,
    OwlNumericDatatype as D, OwlNumericValue, TRIPLE_RELATION,
};

// bounded: sixteen explicitly selected XML Schema datatypes, not graph metadata.
pub(super) const DATATYPES: [D; 16] = [
    D::Decimal,
    D::Integer,
    D::NonPositiveInteger,
    D::NegativeInteger,
    D::Long,
    D::Int,
    D::Short,
    D::Byte,
    D::NonNegativeInteger,
    D::UnsignedLong,
    D::UnsignedInt,
    D::UnsignedShort,
    D::UnsignedByte,
    D::PositiveInteger,
    D::Float,
    D::Double,
];

pub(super) struct Source<'a, G: Graph> {
    pub(super) graph: &'a G,
}
impl<G: Graph> Source<'_, G> {
    pub(super) fn parse(literal: &Literal) -> Result<Option<OwlNumericValue>> {
        let Some(datatype) = DATATYPES
            .iter()
            .find(|datatype| datatype.iri() == literal.datatype().as_str())
        else {
            return Ok(None);
        };
        datatype
            .parse_lexical(literal.value())
            .map(Some)
            .map_err(|error| {
                Error::Runtime(format!(
                    "Invalid standard numeric literal {literal}: {error}"
                ))
            })
    }

    pub(super) fn literals(
        &self,
        bound: Option<Cell>,
        mut visit: impl FnMut(Term, OwlNumericValue) -> Result<()>,
    ) -> Result<()> {
        if let Some(cell) = bound {
            if let Cell::Rdf(Term::Literal(literal)) = cell
                && let Some(value) = Self::parse(&literal)?
            {
                visit(literal.into(), value)?;
            }
            return Ok(());
        }
        for quad in self.graph.quads(None, None, None) {
            let quad = quad?;
            for term in [quad.subject, quad.predicate, quad.object] {
                if let Term::Literal(literal) = term
                    && let Some(value) = Self::parse(&literal)?
                {
                    visit(literal.into(), value)?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn facts(
        &self,
        atom: &Atom,
        binding: &Bindings,
        mut visit: impl FnMut(&[Cell]) -> Result<()>,
    ) -> Result<()> {
        match atom.predicate.as_str() {
            TRIPLE_RELATION => {
                if atom.terms.len() != 3 {
                    return Err(Error::Unsupported(
                        "Numeric triple relation requires three terms".into(),
                    ));
                }
                // bounded: three indexed RDF pattern operands for one clause.
                let mut pattern: [Option<Term>; 3] = [None, None, None];
                for (slot, term) in pattern.iter_mut().zip(&atom.terms) {
                    match binding.bound(term)? {
                        Some(Cell::Rdf(value)) => *slot = Some(value),
                        Some(Cell::Value(_)) => return Ok(()),
                        None => {}
                    }
                }
                for quad in self.graph.quads(
                    pattern[0].as_ref().map(Term::as_ref),
                    pattern[1].as_ref().map(Term::as_ref),
                    pattern[2].as_ref().map(Term::as_ref),
                ) {
                    let quad = quad?;
                    visit(&[
                        Cell::Rdf(quad.subject),
                        Cell::Rdf(quad.predicate),
                        Cell::Rdf(quad.object),
                    ])?;
                }
            }
            OWL_NUMERIC_VALUE_RELATION
            | OWL_NUMERIC_MEMBER_RELATION
            | OWL_NUMERIC_NONMEMBER_RELATION => {
                if atom.terms.len() != 2 {
                    return Err(Error::Unsupported(
                        "Numeric semantic relation requires two terms".into(),
                    ));
                }
                let relation = atom.predicate.as_str();
                self.literals(binding.bound(&atom.terms[0])?, |literal, value| {
                    if relation == OWL_NUMERIC_VALUE_RELATION {
                        return visit(&[Cell::Rdf(literal), Cell::Value(value)]);
                    }
                    for datatype in DATATYPES {
                        if value.is_member_of(datatype) == (relation == OWL_NUMERIC_MEMBER_RELATION)
                        {
                            visit(&[
                                Cell::Rdf(literal.clone()),
                                Cell::Rdf(NamedNode::new_unchecked(datatype.iri()).into()),
                            ])?;
                        }
                    }
                    Ok(())
                })?;
            }
            _ => {
                return Err(Error::Unsupported(
                    "Unsupported numeric semantic relation".into(),
                ));
            }
        }
        Ok(())
    }
}
