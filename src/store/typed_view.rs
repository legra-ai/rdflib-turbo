//! Shared validation over borrowed RDF text; input ownership stays with its caller.
use std::borrow::Cow;

use oxigraph::model::{BlankNodeRef, LiteralRef, NamedNodeRef, NamedOrBlankNodeRef, TermRef};
use oxilangtag::LanguageTag;

use super::operations::invalid;
use crate::{Error, Result};

#[derive(Clone, Copy)]
pub(super) struct TermView<'a> {
    kind: &'a str,
    value: &'a str,
    datatype: Option<&'a str>,
    language: Option<&'a str>,
}

#[derive(Clone, Copy)]
pub(super) enum Position {
    Subject,
    Predicate,
    Object,
}

pub(super) enum Kind {
    Iri,
    Blank,
    Literal,
}

impl<'a> TermView<'a> {
    pub(super) fn new(
        kind: &'a str,
        value: &'a str,
        datatype: Option<&'a str>,
        language: Option<&'a str>,
    ) -> Self {
        Self {
            kind,
            value,
            datatype,
            language,
        }
    }

    pub(super) fn validate(self, position: Position) -> Result<Kind> {
        let kind = match self.kind {
            "uri" if self.datatype.is_none() && self.language.is_none() => Kind::Iri,
            "bnode" if self.datatype.is_none() && self.language.is_none() => Kind::Blank,
            "literal" => {
                if let Some(language) = self.language {
                    if self.datatype.is_some() {
                        return Err(Error::Invalid(
                            "Literal cannot supply datatype and language".into(),
                        ));
                    }
                    // BCP47 is case insensitive; use the same parser as the model constructor.
                    LanguageTag::parse(language).map_err(invalid)?;
                }
                Kind::Literal
            }
            _ => return Err(Error::Invalid("Invalid RDF term tuple".into())),
        };
        match (&kind, position) {
            (_, Position::Object) | (Kind::Iri, _) | (Kind::Blank, Position::Subject) => Ok(kind),
            (_, Position::Subject) => Err(Error::Invalid(
                "Subject must be an IRI or blank node".into(),
            )),
            (_, Position::Predicate) => Err(Error::Invalid("Predicate must be an IRI".into())),
        }
    }
}

pub(super) struct PreparedTerm<'a> {
    source: TermView<'a>,
    kind: Kind,
    language: Option<Cow<'a, str>>,
}

impl<'a> PreparedTerm<'a> {
    pub(super) fn new(source: TermView<'a>, position: Position) -> Result<Self> {
        let kind = source.validate(position)?;
        let language = source.language.map(|language| {
            if language.bytes().any(|byte| byte.is_ascii_uppercase()) {
                Cow::Owned(language.to_ascii_lowercase())
            } else {
                Cow::Borrowed(language)
            }
        });
        Ok(Self {
            source,
            kind,
            language,
        })
    }

    pub(super) fn term_ref(&self) -> TermRef<'_> {
        match self.kind {
            Kind::Iri => NamedNodeRef::new_unchecked(self.source.value).into(),
            Kind::Blank => BlankNodeRef::new_unchecked(self.source.value).into(),
            Kind::Literal => {
                if let Some(language) = &self.language {
                    LiteralRef::new_language_tagged_literal_unchecked(self.source.value, language)
                        .into()
                } else if let Some(datatype) = self.source.datatype {
                    LiteralRef::new_typed_literal(
                        self.source.value,
                        NamedNodeRef::new_unchecked(datatype),
                    )
                    .into()
                } else {
                    LiteralRef::new_simple_literal(self.source.value).into()
                }
            }
        }
    }

    pub(super) fn subject_ref(&self) -> NamedOrBlankNodeRef<'_> {
        match self.term_ref() {
            TermRef::NamedNode(node) => node.into(),
            TermRef::BlankNode(node) => node.into(),
            _ => unreachable!("subject position was validated before preparing this term"),
        }
    }

    pub(super) fn predicate_ref(&self) -> NamedNodeRef<'_> {
        match self.term_ref() {
            TermRef::NamedNode(node) => node,
            _ => unreachable!("predicate position was validated before preparing this term"),
        }
    }
}

#[cfg(test)]
#[path = "typed_view_tests.rs"]
mod tests;
