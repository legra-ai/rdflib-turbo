//! Native CONSTRUCT heads keep immutable lexical owners instead of decoding each row.
use super::dataset::DatasetView;
use crate::model::{BlankNodeRef, GeneralizedTripleRef, LiteralRef, NamedNodeRef, Term, TermRef};
use crate::storage::numeric_encoder::EncodedTerm;
use crate::store::LexicalTerm;
use spareval::{InferenceProjection, InferenceTerm, QueryEvaluationError};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug)]
pub(super) enum HeadTerm {
    Lexical(LexicalTerm),
    Owned(Term),
}
impl HeadTerm {
    fn as_ref(&self) -> TermRef<'_> {
        match self {
            Self::Owned(term) => term.as_ref(),
            Self::Lexical(term) => match term {
                LexicalTerm::NamedNode(value) => NamedNodeRef::new_unchecked(value.as_str()).into(),
                LexicalTerm::BlankNode(value) => BlankNodeRef::new_unchecked(value.as_str()).into(),
                LexicalTerm::Literal {
                    value,
                    datatype,
                    language,
                    ..
                } => {
                    #[cfg(feature = "rdf-12")]
                    if let LexicalTerm::Literal {
                        direction: Some(direction),
                        ..
                    } = term
                    {
                        return LiteralRef::new_directional_language_tagged_literal_unchecked(
                            value.as_str(),
                            language
                                .as_ref()
                                .expect("direction requires language")
                                .as_str(),
                            *direction,
                        )
                        .into();
                    }
                    if let Some(language) = language {
                        LiteralRef::new_language_tagged_literal_unchecked(
                            value.as_str(),
                            language.as_str(),
                        )
                        .into()
                    } else if let Some(datatype) = datatype {
                        LiteralRef::new_typed_literal(
                            value.as_str(),
                            NamedNodeRef::new_unchecked(datatype.as_str()),
                        )
                        .into()
                    } else {
                        LiteralRef::new_simple_literal(value.as_str()).into()
                    }
                }
                #[cfg(feature = "rdf-12")]
                LexicalTerm::Triple(_) => {
                    unreachable!("quoted heads retain their checked model owner")
                }
            },
        }
    }
}

/// A full-Term inference head with independent immutable lexical owners.
/// Native identities never escape their query cache; comparisons use canonical RDF terms.
#[derive(Clone, Debug)]
pub struct LeasedInferenceTriple([Arc<HeadTerm>; 3]);
impl LeasedInferenceTriple {
    /// Borrows all three canonical terms without decoding or copying their lexical payloads.
    pub fn as_ref(&self) -> GeneralizedTripleRef<'_> {
        GeneralizedTripleRef {
            subject: self.0[0].as_ref().as_ref(),
            predicate: self.0[1].as_ref().as_ref(),
            object: self.0[2].as_ref().as_ref(),
        }
    }
}

#[derive(Default)]
pub(super) struct Projection {
    // Bounded by the distinct emitted terms of the existing query result cache;
    // retired with that cache. Arc owners already in Pending remain independently alive.
    native: HashMap<EncodedTerm, Arc<HeadTerm>>,
    // Bounded by immutable CONSTRUCT template constants, never by solution cardinality.
    constants: HashMap<Term, Arc<HeadTerm>>,
}
impl Projection {
    fn term(
        &mut self,
        term: &InferenceTerm<'_, EncodedTerm>,
        dataset: &DatasetView<'_>,
    ) -> Result<Arc<HeadTerm>, QueryEvaluationError> {
        Ok(match term {
            InferenceTerm::Internal(id) => {
                if let Some(owner) = self.native.get(id) {
                    return Ok(Arc::clone(owner));
                }
                let owner = Arc::new(
                    dataset
                        .lease_term(id)
                        .map_err(|error| QueryEvaluationError::Dataset(Box::new(error)))?,
                );
                if self.native.len() >= 3 * 1024 * 1024 {
                    self.native.clear();
                }
                self.native.insert(id.clone(), Arc::clone(&owner));
                owner
            }
            InferenceTerm::Constant(term) => {
                if let Some(owner) = self.constants.get(*term) {
                    return Ok(Arc::clone(owner));
                }
                let owner = Arc::new(HeadTerm::Owned((*term).clone()));
                self.constants.insert((*term).clone(), Arc::clone(&owner));
                owner
            }
            InferenceTerm::External(term) => Arc::new(HeadTerm::Owned(term.clone())),
        })
    }
}
impl<'a> InferenceProjection<'a, DatasetView<'a>> for Projection {
    type Output = LeasedInferenceTriple;
    fn project(
        &mut self,
        terms: &[InferenceTerm<'_, EncodedTerm>; 3],
        dataset: &DatasetView<'a>,
    ) -> Result<Self::Output, QueryEvaluationError> {
        Ok(LeasedInferenceTriple([
            self.term(&terms[0], dataset)?,
            self.term(&terms[1], dataset)?,
            self.term(&terms[2], dataset)?,
        ]))
    }
    fn terms(output: &Self::Output) -> [TermRef<'_>; 3] {
        let head = output.as_ref();
        [head.subject, head.predicate, head.object]
    }
    fn clear(&mut self) {
        self.native.clear();
    }
}
