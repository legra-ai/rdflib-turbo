//! Project lexical owners without constructing and then dismantling RDF model objects.
use super::{QuadIter, StorageError};
#[cfg(feature = "rdf-12")]
use crate::model::BaseDirection;
use crate::model::{BlankNode, Term};
use crate::storage::numeric_encoder::{Decoder, EncodedQuad, EncodedTerm};
use crate::storage::term_text::TermText;
use crate::storage::{CorruptionError, StorageReader};

/// A selected RDF term whose lexical payloads have independent immutable owners.
#[derive(Clone, Debug)]
pub enum LexicalTerm {
    /// An IRI, including unchecked relative IRIs already accepted by storage.
    NamedNode(TermText),
    /// A blank-node identifier.
    BlankNode(TermText),
    /// A literal preserving lexical spelling and explicit datatype/language identity.
    Literal {
        /// Lexical value, without canonicalization beyond the encoded source representation.
        value: TermText,
        /// Explicit datatype; absent for simple and language-tagged strings.
        datatype: Option<TermText>,
        /// Language tag, when present.
        language: Option<TermText>,
        /// RDF 1.2 base direction, when present.
        #[cfg(feature = "rdf-12")]
        direction: Option<BaseDirection>,
    },
    /// An RDF 1.2 triple term in subject, predicate, object order.
    #[cfg(feature = "rdf-12")]
    Triple(Box<[LexicalTerm; 3]>),
}

impl LexicalTerm {
    fn literal(value: TermText, datatype: Option<TermText>, language: Option<TermText>) -> Self {
        Self::Literal {
            value,
            datatype,
            language,
            #[cfg(feature = "rdf-12")]
            direction: None,
        }
    }

    #[cfg(feature = "rdf-12")]
    fn directional(value: TermText, language: TermText, direction: BaseDirection) -> Self {
        Self::Literal {
            value,
            datatype: None,
            language: Some(language),
            direction: Some(direction),
        }
    }

    fn decode_position(
        reader: &StorageReader<'_>,
        encoded: &EncodedTerm,
        position: usize,
    ) -> Result<Self, StorageError> {
        let term = Self::decode(reader, encoded)?;
        match (&term, position) {
            (Self::NamedNode(_), _) | (Self::BlankNode(_), 0) | (_, 2) => Ok(term),
            _ => {
                Err(CorruptionError::msg("Invalid RDF term in selected statement position").into())
            }
        }
    }

    pub(super) fn decode(
        reader: &StorageReader<'_>,
        encoded: &EncodedTerm,
    ) -> Result<Self, StorageError> {
        Self::decode_with(reader, encoded, &|id| reader.get_term_text(id))
    }

    pub(crate) fn decode_with(
        reader: &StorageReader<'_>,
        encoded: &EncodedTerm,
        lookup: &impl Fn(&crate::storage::numeric_encoder::StrHash) -> Result<TermText, StorageError>,
    ) -> Result<Self, StorageError> {
        use EncodedTerm::*;
        Ok(match encoded {
            DefaultGraph => {
                return Err(
                    CorruptionError::msg("The default graph tag is not a valid term").into(),
                );
            }
            NamedNode { iri_id } => Self::NamedNode(lookup(iri_id)?),
            NumericalBlankNode { id } => Self::BlankNode(TermText::owned(
                BlankNode::new_from_unique_id(u128::from_be_bytes(*id)).into_string(),
            )),
            SmallBlankNode(id) => Self::BlankNode(TermText::inline(*id)),
            BigBlankNode { id_id } => Self::BlankNode(lookup(id_id)?),
            SmallStringLiteral(value) => Self::literal(TermText::inline(*value), None, None),
            BigStringLiteral { value_id } => Self::literal(lookup(value_id)?, None, None),
            SmallSmallLangStringLiteral { value, language } => Self::literal(
                TermText::inline(*value),
                None,
                Some(TermText::inline(*language)),
            ),
            SmallBigLangStringLiteral { value, language_id } => {
                Self::literal(TermText::inline(*value), None, Some(lookup(language_id)?))
            }
            BigSmallLangStringLiteral { value_id, language } => {
                Self::literal(lookup(value_id)?, None, Some(TermText::inline(*language)))
            }
            BigBigLangStringLiteral {
                value_id,
                language_id,
            } => Self::literal(lookup(value_id)?, None, Some(lookup(language_id)?)),
            SmallTypedLiteral { value, datatype_id } => {
                Self::literal(TermText::inline(*value), Some(lookup(datatype_id)?), None)
            }
            BigTypedLiteral {
                value_id,
                datatype_id,
            } => Self::literal(lookup(value_id)?, Some(lookup(datatype_id)?), None),
            BooleanLiteral(_)
            | FloatLiteral(_)
            | DoubleLiteral(_)
            | IntegerLiteral(_)
            | DecimalLiteral(_)
            | DateTimeLiteral(_)
            | TimeLiteral(_)
            | DateLiteral(_)
            | GYearMonthLiteral(_)
            | GYearLiteral(_)
            | GMonthDayLiteral(_)
            | GDayLiteral(_)
            | GMonthLiteral(_)
            | DurationLiteral(_)
            | YearMonthDurationLiteral(_)
            | DayTimeDurationLiteral(_) => {
                // These native scalar encodings have no stored lexical payload to lease.
                // The existing formatter owns the canonical lexical string; move it once.
                let Term::Literal(literal) = reader.decode_term(encoded)? else {
                    unreachable!("native literal encodings always decode to literals")
                };
                #[cfg(not(feature = "rdf-12"))]
                let (value, datatype, language) = literal.destruct();
                #[cfg(feature = "rdf-12")]
                let (value, datatype, language, direction) = literal.destruct();
                Self::Literal {
                    value: TermText::owned(value),
                    datatype: datatype.map(|value| TermText::owned(value.into_string())),
                    language: language.map(TermText::owned),
                    #[cfg(feature = "rdf-12")]
                    direction,
                }
            }
            #[cfg(feature = "rdf-12")]
            LtrSmallSmallDirLangStringLiteral { value, language } => Self::directional(
                TermText::inline(*value),
                TermText::inline(*language),
                BaseDirection::Ltr,
            ),
            #[cfg(feature = "rdf-12")]
            LtrSmallBigDirLangStringLiteral { value, language_id } => Self::directional(
                TermText::inline(*value),
                lookup(language_id)?,
                BaseDirection::Ltr,
            ),
            #[cfg(feature = "rdf-12")]
            LtrBigSmallDirLangStringLiteral { value_id, language } => Self::directional(
                lookup(value_id)?,
                TermText::inline(*language),
                BaseDirection::Ltr,
            ),
            #[cfg(feature = "rdf-12")]
            LtrBigBigDirLangStringLiteral {
                value_id,
                language_id,
            } => Self::directional(lookup(value_id)?, lookup(language_id)?, BaseDirection::Ltr),
            #[cfg(feature = "rdf-12")]
            RtlSmallSmallDirLangStringLiteral { value, language } => Self::directional(
                TermText::inline(*value),
                TermText::inline(*language),
                BaseDirection::Rtl,
            ),
            #[cfg(feature = "rdf-12")]
            RtlSmallBigDirLangStringLiteral { value, language_id } => Self::directional(
                TermText::inline(*value),
                lookup(language_id)?,
                BaseDirection::Rtl,
            ),
            #[cfg(feature = "rdf-12")]
            RtlBigSmallDirLangStringLiteral { value_id, language } => Self::directional(
                lookup(value_id)?,
                TermText::inline(*language),
                BaseDirection::Rtl,
            ),
            #[cfg(feature = "rdf-12")]
            RtlBigBigDirLangStringLiteral {
                value_id,
                language_id,
            } => Self::directional(lookup(value_id)?, lookup(language_id)?, BaseDirection::Rtl),
            #[cfg(feature = "rdf-12")]
            Triple(triple) => Self::Triple(Box::new([
                Self::decode_position(reader, &triple.subject, 0)?,
                Self::decode_position(reader, &triple.predicate, 1)?,
                Self::decode_position(reader, &triple.object, 2)?,
            ])),
        })
    }
}

/// A projected quad cursor leasing immutable lexical payloads from its existing snapshot.
///
/// Selection and errors follow `QuadIter::project`. The original reader and borrowing
/// lifetime are retained; graph names and unselected fields are never decoded.
#[must_use]
pub struct LexicalQuadIter<'a> {
    inner: QuadIter<'a>,
    selected: [bool; 3],
}

impl<'a> QuadIter<'a> {
    /// Consumes this cursor and yields selected RDF terms with independent lexical owners.
    ///
    /// Memory dictionary text is shared without a lexical copy; persistent dictionary
    /// text owns its decoded storage value. Numeric encodings own formatter output.
    /// No dictionary guard escapes, and no additional snapshot is created.
    pub fn project_lexical(
        self,
        subject: bool,
        predicate: bool,
        object: bool,
    ) -> LexicalQuadIter<'a> {
        LexicalQuadIter {
            inner: self,
            selected: [subject, predicate, object],
        }
    }
}

/// Share field selection while preserving each cursor's explicit term domain.
pub(super) fn project<const GENERALIZED: bool>(
    reader: &StorageReader<'_>,
    quad: &EncodedQuad,
    selected: [bool; 3],
) -> Result<[Option<LexicalTerm>; 3], StorageError> {
    let decode = |encoded: &EncodedTerm, position| {
        if GENERALIZED {
            LexicalTerm::decode(reader, encoded)
        } else {
            LexicalTerm::decode_position(reader, encoded, position)
        }
    };
    Ok([
        selected[0].then(|| decode(&quad.subject, 0)).transpose()?,
        selected[1]
            .then(|| decode(&quad.predicate, 1))
            .transpose()?,
        selected[2].then(|| decode(&quad.object, 2)).transpose()?,
    ])
}

impl Iterator for LexicalQuadIter<'_> {
    type Item = Result<[Option<LexicalTerm>; 3], StorageError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner
            .iter
            .next()
            .map(|quad| project::<false>(&self.inner.reader, &quad?, self.selected))
    }
}
