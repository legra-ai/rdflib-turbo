use crate::named_node::{NamedNode, NamedNodeRef};
use crate::vocab::{rdf, xsd};
use oxilangtag::{LanguageTag, LanguageTagParseError};
#[cfg(feature = "oxsdatatypes")]
use oxsdatatypes::*;
#[cfg(feature = "serde")]
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::borrow::Cow;
use std::fmt;
use std::fmt::Write;

/// An owned RDF [literal](https://www.w3.org/TR/rdf11-concepts/#dfn-literal).
///
/// The default string formatter is returning an N-Triples, Turtle, and SPARQL compatible representation:
/// ```
/// # use oxilangtag::LanguageTagParseError;
/// use oxrdf::Literal;
/// use oxrdf::vocab::xsd;
///
/// assert_eq!(
///     "\"foo\\nbar\"",
///     Literal::new_simple_literal("foo\nbar").to_string()
/// );
///
/// assert_eq!(
///     r#""1999-01-01"^^<http://www.w3.org/2001/XMLSchema#date>"#,
///     Literal::new_typed_literal("1999-01-01", xsd::DATE).to_string()
/// );
///
/// assert_eq!(
///     r#""foo"@en"#,
///     Literal::new_language_tagged_literal("foo", "en")?.to_string()
/// );
/// # Result::<_, LanguageTagParseError>::Ok(())
/// ```
#[derive(Eq, PartialEq, Debug, Clone, Hash)]
pub struct Literal(LiteralContent);

#[derive(PartialEq, Eq, Debug, Clone, Hash)]
enum LiteralContent {
    String(String),
    LanguageTaggedString {
        value: String,
        language: String,
    },
    #[cfg(feature = "rdf-12")]
    DirectionalLanguageTaggedString {
        value: String,
        language: String,
        direction: BaseDirection,
    },
    TypedLiteral {
        value: String,
        datatype: NamedNode,
    },
}

impl Literal {
    /// Builds an RDF [simple literal](https://www.w3.org/TR/rdf11-concepts/#dfn-simple-literal).
    #[inline]
    pub fn new_simple_literal(value: impl Into<String>) -> Self {
        Self(LiteralContent::String(value.into()))
    }

    /// Builds an RDF [literal](https://www.w3.org/TR/rdf11-concepts/#dfn-literal) with a [datatype](https://www.w3.org/TR/rdf11-concepts/#dfn-datatype-iri).
    #[inline]
    pub fn new_typed_literal(value: impl Into<String>, datatype: impl Into<NamedNode>) -> Self {
        let value = value.into();
        let datatype = datatype.into();
        Self(
            if !cfg!(feature = "rdf-10-string-identity") && datatype == xsd::STRING {
                LiteralContent::String(value)
            } else {
                LiteralContent::TypedLiteral { value, datatype }
            },
        )
    }

    /// Builds an RDF [language-tagged string](https://www.w3.org/TR/rdf11-concepts/#dfn-language-tagged-string).
    #[inline]
    pub fn new_language_tagged_literal(
        value: impl Into<String>,
        language: impl Into<String>,
    ) -> Result<Self, LanguageTagParseError> {
        let mut language = language.into();
        language.make_ascii_lowercase();
        Ok(Self::new_language_tagged_literal_unchecked(
            value,
            LanguageTag::parse(language)?.into_inner(),
        ))
    }

    /// Builds an RDF [language-tagged string](https://www.w3.org/TR/rdf11-concepts/#dfn-language-tagged-string).
    ///
    /// It is the responsibility of the caller to check that `language`
    /// is valid [BCP47](https://tools.ietf.org/html/bcp47) language tag,
    /// and is lowercase.
    ///
    /// [`Literal::new_language_tagged_literal()`] is a safe version of this constructor and should be used for untrusted data.
    #[inline]
    pub fn new_language_tagged_literal_unchecked(
        value: impl Into<String>,
        language: impl Into<String>,
    ) -> Self {
        Self(LiteralContent::LanguageTaggedString {
            value: value.into(),
            language: language.into(),
        })
    }

    /// Builds an RDF [directional language-tagged string](https://www.w3.org/TR/rdf12-concepts/#dfn-dir-lang-string).
    #[cfg(feature = "rdf-12")]
    #[inline]
    pub fn new_directional_language_tagged_literal(
        value: impl Into<String>,
        language: impl Into<String>,
        direction: impl Into<BaseDirection>,
    ) -> Result<Self, LanguageTagParseError> {
        let mut language = language.into();
        language.make_ascii_lowercase();
        Ok(Self::new_directional_language_tagged_literal_unchecked(
            value,
            LanguageTag::parse(language)?.into_inner(),
            direction,
        ))
    }

    /// Builds an RDF [directional language-tagged string](https://www.w3.org/TR/rdf12-concepts/#dfn-dir-lang-string).
    ///
    /// It is the responsibility of the caller to check that `language`
    /// is valid [BCP47](https://tools.ietf.org/html/bcp47) language tag,
    /// and is lowercase.
    ///
    /// [`Literal::new_language_tagged_literal()`] is a safe version of this constructor and should be used for untrusted data.
    #[cfg(feature = "rdf-12")]
    #[inline]
    pub fn new_directional_language_tagged_literal_unchecked(
        value: impl Into<String>,
        language: impl Into<String>,
        direction: impl Into<BaseDirection>,
    ) -> Self {
        Self(LiteralContent::DirectionalLanguageTaggedString {
            value: value.into(),
            language: language.into(),
            direction: direction.into(),
        })
    }

    /// The literal [lexical form](https://www.w3.org/TR/rdf11-concepts/#dfn-lexical-form).
    #[inline]
    pub fn value(&self) -> &str {
        self.as_ref().value()
    }

    /// The literal [language tag](https://www.w3.org/TR/rdf11-concepts/#dfn-language-tag) if it is a [language-tagged string](https://www.w3.org/TR/rdf11-concepts/#dfn-language-tagged-string).
    ///
    /// Language tags are defined by the [BCP47](https://tools.ietf.org/html/bcp47).
    /// They are normalized to lowercase by this implementation.
    #[inline]
    pub fn language(&self) -> Option<&str> {
        self.as_ref().language()
    }

    /// The literal [base direction](https://www.w3.org/TR/rdf12-concepts/#dfn-base-direction) if it is a [directional language-tagged string](https://www.w3.org/TR/rdf12-concepts/#dfn-base-direction).
    ///
    /// The two possible base directions are left-to-right (`ltr`) and right-to-left (`rtl`).
    #[cfg(feature = "rdf-12")]
    #[inline]
    pub fn direction(&self) -> Option<BaseDirection> {
        self.as_ref().direction()
    }

    /// The literal [datatype](https://www.w3.org/TR/rdf11-concepts/#dfn-datatype-iri).
    ///
    /// The datatype of [language-tagged string](https://www.w3.org/TR/rdf11-concepts/#dfn-language-tagged-string) is always [rdf:langString](https://www.w3.org/TR/rdf11-concepts/#dfn-language-tagged-string).
    /// The datatype of [simple literals](https://www.w3.org/TR/rdf11-concepts/#dfn-simple-literal) is [xsd:string](https://www.w3.org/TR/xmlschema11-2/#string).
    #[inline]
    pub fn datatype(&self) -> NamedNodeRef<'_> {
        self.as_ref().datatype()
    }

    /// Whether the literal was constructed with an explicit datatype.
    /// With `rdf-10-string-identity`, explicit xsd:string remains distinct.
    #[inline]
    pub fn has_explicit_datatype(&self) -> bool {
        self.as_ref().has_explicit_datatype()
    }

    /// Checks if this literal could be seen as an RDF 1.0 [plain literal](https://www.w3.org/TR/2004/REC-rdf-concepts-20040210/#dfn-plain-literal).
    ///
    /// It returns true if the literal is a [language-tagged string](https://www.w3.org/TR/rdf11-concepts/#dfn-language-tagged-string)
    /// or has the datatype [xsd:string](https://www.w3.org/TR/xmlschema11-2/#string).
    #[inline]
    #[deprecated(note = "Plain literal concept is removed in RDF 1.1", since = "0.3.0")]
    pub fn is_plain(&self) -> bool {
        #[expect(deprecated)]
        self.as_ref().is_plain()
    }

    #[inline]
    pub fn as_ref(&self) -> LiteralRef<'_> {
        LiteralRef(match &self.0 {
            LiteralContent::String(value) => LiteralRefContent::String(value),
            LiteralContent::LanguageTaggedString { value, language } => {
                LiteralRefContent::LanguageTaggedString { value, language }
            }
            #[cfg(feature = "rdf-12")]
            LiteralContent::DirectionalLanguageTaggedString {
                value,
                language,
                direction,
            } => LiteralRefContent::DirectionalLanguageTaggedString {
                value,
                language,
                direction: *direction,
            },
            LiteralContent::TypedLiteral { value, datatype } => LiteralRefContent::TypedLiteral {
                value,
                datatype: datatype.as_ref(),
            },
        })
    }

    /// Extract components from this literal (value, datatype, language tag and base direction).
    #[cfg(feature = "rdf-12")]
    #[inline]
    pub fn destruct(
        self,
    ) -> (
        String,
        Option<NamedNode>,
        Option<String>,
        Option<BaseDirection>,
    ) {
        match self.0 {
            LiteralContent::String(s) => (s, None, None, None),
            LiteralContent::LanguageTaggedString { value, language } => {
                (value, None, Some(language), None)
            }
            LiteralContent::DirectionalLanguageTaggedString {
                value,
                language,
                direction,
            } => (value, None, Some(language), Some(direction)),
            LiteralContent::TypedLiteral { value, datatype } => (value, Some(datatype), None, None),
        }
    }

    /// Extract components from this literal (value, datatype and language tag).
    #[cfg(not(feature = "rdf-12"))]
    #[inline]
    pub fn destruct(self) -> (String, Option<NamedNode>, Option<String>) {
        match self.0 {
            LiteralContent::String(s) => (s, None, None),
            LiteralContent::LanguageTaggedString { value, language } => {
                (value, None, Some(language))
            }
            LiteralContent::TypedLiteral { value, datatype } => (value, Some(datatype), None),
        }
    }
}

impl fmt::Display for Literal {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl<'a> From<&'a str> for Literal {
    #[inline]
    fn from(value: &'a str) -> Self {
        Self(LiteralContent::String(value.into()))
    }
}

impl From<String> for Literal {
    #[inline]
    fn from(value: String) -> Self {
        Self(LiteralContent::String(value))
    }
}

impl<'a> From<Cow<'a, str>> for Literal {
    #[inline]
    fn from(value: Cow<'a, str>) -> Self {
        Self(LiteralContent::String(value.into()))
    }
}

impl From<bool> for Literal {
    #[inline]
    fn from(value: bool) -> Self {
        Self(LiteralContent::TypedLiteral {
            value: value.to_string(),
            datatype: xsd::BOOLEAN.into(),
        })
    }
}

impl From<i128> for Literal {
    #[inline]
    fn from(value: i128) -> Self {
        Self(LiteralContent::TypedLiteral {
            value: value.to_string(),
            datatype: xsd::INTEGER.into(),
        })
    }
}

impl From<i64> for Literal {
    #[inline]
    fn from(value: i64) -> Self {
        Self(LiteralContent::TypedLiteral {
            value: value.to_string(),
            datatype: xsd::INTEGER.into(),
        })
    }
}

impl From<i32> for Literal {
    #[inline]
    fn from(value: i32) -> Self {
        Self(LiteralContent::TypedLiteral {
            value: value.to_string(),
            datatype: xsd::INTEGER.into(),
        })
    }
}

impl From<i16> for Literal {
    #[inline]
    fn from(value: i16) -> Self {
        Self(LiteralContent::TypedLiteral {
            value: value.to_string(),
            datatype: xsd::INTEGER.into(),
        })
    }
}

impl From<u64> for Literal {
    #[inline]
    fn from(value: u64) -> Self {
        Self(LiteralContent::TypedLiteral {
            value: value.to_string(),
            datatype: xsd::INTEGER.into(),
        })
    }
}

impl From<u32> for Literal {
    #[inline]
    fn from(value: u32) -> Self {
        Self(LiteralContent::TypedLiteral {
            value: value.to_string(),
            datatype: xsd::INTEGER.into(),
        })
    }
}

impl From<u16> for Literal {
    #[inline]
    fn from(value: u16) -> Self {
        Self(LiteralContent::TypedLiteral {
            value: value.to_string(),
            datatype: xsd::INTEGER.into(),
        })
    }
}

impl From<f32> for Literal {
    #[inline]
    fn from(value: f32) -> Self {
        Self(LiteralContent::TypedLiteral {
            value: if value == f32::INFINITY {
                "INF".to_owned()
            } else if value == f32::NEG_INFINITY {
                "-INF".to_owned()
            } else {
                value.to_string()
            },
            datatype: xsd::FLOAT.into(),
        })
    }
}

impl From<f64> for Literal {
    #[inline]
    fn from(value: f64) -> Self {
        Self(LiteralContent::TypedLiteral {
            value: if value == f64::INFINITY {
                "INF".to_owned()
            } else if value == f64::NEG_INFINITY {
                "-INF".to_owned()
            } else {
                value.to_string()
            },
            datatype: xsd::DOUBLE.into(),
        })
    }
}

#[cfg(feature = "oxsdatatypes")]
impl From<Boolean> for Literal {
    #[inline]
    fn from(value: Boolean) -> Self {
        Self::new_typed_literal(value.to_string(), xsd::BOOLEAN)
    }
}

#[cfg(feature = "oxsdatatypes")]
impl From<Float> for Literal {
    #[inline]
    fn from(value: Float) -> Self {
        Self::new_typed_literal(value.to_string(), xsd::FLOAT)
    }
}

#[cfg(feature = "oxsdatatypes")]
impl From<Double> for Literal {
    #[inline]
    fn from(value: Double) -> Self {
        Self::new_typed_literal(value.to_string(), xsd::DOUBLE)
    }
}

#[cfg(feature = "oxsdatatypes")]
impl From<Integer> for Literal {
    #[inline]
    fn from(value: Integer) -> Self {
        Self::new_typed_literal(value.to_string(), xsd::INTEGER)
    }
}

#[cfg(feature = "oxsdatatypes")]
impl From<Decimal> for Literal {
    #[inline]
    fn from(value: Decimal) -> Self {
        Self::new_typed_literal(value.to_string(), xsd::DECIMAL)
    }
}

#[cfg(feature = "oxsdatatypes")]
impl From<DateTime> for Literal {
    #[inline]
    fn from(value: DateTime) -> Self {
        Self::new_typed_literal(value.to_string(), xsd::DATE_TIME)
    }
}

#[cfg(feature = "oxsdatatypes")]
impl From<Time> for Literal {
    #[inline]
    fn from(value: Time) -> Self {
        Self::new_typed_literal(value.to_string(), xsd::TIME)
    }
}

#[cfg(feature = "oxsdatatypes")]
impl From<Date> for Literal {
    #[inline]
    fn from(value: Date) -> Self {
        Self::new_typed_literal(value.to_string(), xsd::DATE)
    }
}

#[cfg(feature = "oxsdatatypes")]
impl From<GYearMonth> for Literal {
    #[inline]
    fn from(value: GYearMonth) -> Self {
        Self::new_typed_literal(value.to_string(), xsd::G_YEAR_MONTH)
    }
}

#[cfg(feature = "oxsdatatypes")]
impl From<GYear> for Literal {
    #[inline]
    fn from(value: GYear) -> Self {
        Self::new_typed_literal(value.to_string(), xsd::G_YEAR)
    }
}

#[cfg(feature = "oxsdatatypes")]
impl From<GMonthDay> for Literal {
    #[inline]
    fn from(value: GMonthDay) -> Self {
        Self::new_typed_literal(value.to_string(), xsd::G_MONTH_DAY)
    }
}

#[cfg(feature = "oxsdatatypes")]
impl From<GMonth> for Literal {
    #[inline]
    fn from(value: GMonth) -> Self {
        Self::new_typed_literal(value.to_string(), xsd::G_MONTH)
    }
}

#[cfg(feature = "oxsdatatypes")]
impl From<GDay> for Literal {
    #[inline]
    fn from(value: GDay) -> Self {
        Self::new_typed_literal(value.to_string(), xsd::G_DAY)
    }
}

#[cfg(feature = "oxsdatatypes")]
impl From<Duration> for Literal {
    #[inline]
    fn from(value: Duration) -> Self {
        Self::new_typed_literal(value.to_string(), xsd::DURATION)
    }
}

#[cfg(feature = "oxsdatatypes")]
impl From<YearMonthDuration> for Literal {
    #[inline]
    fn from(value: YearMonthDuration) -> Self {
        Self::new_typed_literal(value.to_string(), xsd::YEAR_MONTH_DURATION)
    }
}

#[cfg(feature = "oxsdatatypes")]
impl From<DayTimeDuration> for Literal {
    #[inline]
    fn from(value: DayTimeDuration) -> Self {
        Self::new_typed_literal(value.to_string(), xsd::DAY_TIME_DURATION)
    }
}

/// A borrowed RDF [literal](https://www.w3.org/TR/rdf11-concepts/#dfn-literal).
///
/// The default string formatter is returning an N-Triples, Turtle, and SPARQL compatible representation:
/// ```
/// use oxrdf::LiteralRef;
/// use oxrdf::vocab::xsd;
///
/// assert_eq!(
///     "\"foo\\nbar\"",
///     LiteralRef::new_simple_literal("foo\nbar").to_string()
/// );
///
/// assert_eq!(
///     r#""1999-01-01"^^<http://www.w3.org/2001/XMLSchema#date>"#,
///     LiteralRef::new_typed_literal("1999-01-01", xsd::DATE).to_string()
/// );
/// ```
#[derive(Eq, PartialEq, Debug, Clone, Copy, Hash)]
pub struct LiteralRef<'a>(LiteralRefContent<'a>);

#[derive(PartialEq, Eq, Debug, Clone, Copy, Hash)]
enum LiteralRefContent<'a> {
    String(&'a str),
    LanguageTaggedString {
        value: &'a str,
        language: &'a str,
    },
    #[cfg(feature = "rdf-12")]
    DirectionalLanguageTaggedString {
        value: &'a str,
        language: &'a str,
        direction: BaseDirection,
    },
    TypedLiteral {
        value: &'a str,
        datatype: NamedNodeRef<'a>,
    },
}

impl<'a> LiteralRef<'a> {
    /// Builds an RDF [simple literal](https://www.w3.org/TR/rdf11-concepts/#dfn-simple-literal).
    #[inline]
    pub const fn new_simple_literal(value: &'a str) -> Self {
        LiteralRef(LiteralRefContent::String(value))
    }

    /// Builds an RDF [literal](https://www.w3.org/TR/rdf11-concepts/#dfn-literal) with a [datatype](https://www.w3.org/TR/rdf11-concepts/#dfn-datatype-iri).
    #[inline]
    pub fn new_typed_literal(value: &'a str, datatype: impl Into<NamedNodeRef<'a>>) -> Self {
        let datatype = datatype.into();
        LiteralRef(
            if !cfg!(feature = "rdf-10-string-identity") && datatype == xsd::STRING {
                LiteralRefContent::String(value)
            } else {
                LiteralRefContent::TypedLiteral { value, datatype }
            },
        )
    }

    /// Builds an RDF [language-tagged string](https://www.w3.org/TR/rdf11-concepts/#dfn-language-tagged-string).
    ///
    /// It is the responsibility of the caller to check that `language`
    /// is valid [BCP47](https://tools.ietf.org/html/bcp47) language tag,
    /// and is lowercase.
    ///
    /// [`Literal::new_language_tagged_literal()`] is a safe version of this constructor and should be used for untrusted data.
    #[inline]
    pub const fn new_language_tagged_literal_unchecked(value: &'a str, language: &'a str) -> Self {
        LiteralRef(LiteralRefContent::LanguageTaggedString { value, language })
    }

    /// Builds an RDF [directional language-tagged string](https://www.w3.org/TR/rdf12-concepts/#dfn-dir-lang-string).
    ///
    /// It is the responsibility of the caller to check that `language`
    /// is valid [BCP47](https://tools.ietf.org/html/bcp47) language tag,
    /// and is lowercase.
    ///
    /// [`Literal::new_directional_language_tagged_literal()`] is a safe version of this constructor and should be used for untrusted data.
    #[cfg(feature = "rdf-12")]
    #[inline]
    pub const fn new_directional_language_tagged_literal_unchecked(
        value: &'a str,
        language: &'a str,
        direction: BaseDirection,
    ) -> Self {
        LiteralRef(LiteralRefContent::DirectionalLanguageTaggedString {
            value,
            language,
            direction,
        })
    }

    /// The literal [lexical form](https://www.w3.org/TR/rdf11-concepts/#dfn-lexical-form)
    #[inline]
    pub const fn value(self) -> &'a str {
        match self.0 {
            LiteralRefContent::String(value)
            | LiteralRefContent::LanguageTaggedString { value, .. }
            | LiteralRefContent::TypedLiteral { value, .. } => value,
            #[cfg(feature = "rdf-12")]
            LiteralRefContent::DirectionalLanguageTaggedString { value, .. } => value,
        }
    }

    /// The literal [language tag](https://www.w3.org/TR/rdf11-concepts/#dfn-language-tag) if it is a [language-tagged string](https://www.w3.org/TR/rdf11-concepts/#dfn-language-tagged-string).
    ///
    /// Language tags are defined by the [BCP47](https://tools.ietf.org/html/bcp47).
    /// They are normalized to lowercase by this implementation.
    #[inline]
    pub const fn language(self) -> Option<&'a str> {
        match self.0 {
            LiteralRefContent::LanguageTaggedString { language, .. } => Some(language),
            #[cfg(feature = "rdf-12")]
            LiteralRefContent::DirectionalLanguageTaggedString { language, .. } => Some(language),
            _ => None,
        }
    }

    /// The literal [base direction](https://www.w3.org/TR/rdf12-concepts/#dfn-base-direction) if it is a [directional language-tagged string](https://www.w3.org/TR/rdf12-concepts/#dfn-base-direction).
    ///
    /// The two possible base directions are left-to-right (`ltr`) and right-to-left (`rtl`).
    #[cfg(feature = "rdf-12")]
    #[inline]
    pub const fn direction(self) -> Option<BaseDirection> {
        match self.0 {
            LiteralRefContent::DirectionalLanguageTaggedString { direction, .. } => Some(direction),
            _ => None,
        }
    }

    /// The literal [datatype](https://www.w3.org/TR/rdf11-concepts/#dfn-datatype-iri).
    ///
    /// The datatype of [language-tagged string](https://www.w3.org/TR/rdf11-concepts/#dfn-language-tagged-string) is always [rdf:langString](https://www.w3.org/TR/rdf11-concepts/#dfn-language-tagged-string).
    /// The datatype of [simple literals](https://www.w3.org/TR/rdf11-concepts/#dfn-simple-literal) is [xsd:string](https://www.w3.org/TR/xmlschema11-2/#string).
    #[inline]
    pub const fn datatype(self) -> NamedNodeRef<'a> {
        match self.0 {
            LiteralRefContent::String(_) => xsd::STRING,
            LiteralRefContent::LanguageTaggedString { .. } => rdf::LANG_STRING,
            #[cfg(feature = "rdf-12")]
            LiteralRefContent::DirectionalLanguageTaggedString { .. } => rdf::DIR_LANG_STRING,
            LiteralRefContent::TypedLiteral { datatype, .. } => datatype,
        }
    }

    /// Whether the literal was constructed with an explicit datatype.
    #[inline]
    pub const fn has_explicit_datatype(self) -> bool {
        matches!(self.0, LiteralRefContent::TypedLiteral { .. })
    }

    /// Checks if this literal could be seen as an RDF 1.0 [plain literal](https://www.w3.org/TR/2004/REC-rdf-concepts-20040210/#dfn-plain-literal).
    ///
    /// It returns true if the literal is a [language-tagged string](https://www.w3.org/TR/rdf11-concepts/#dfn-language-tagged-string)
    /// or has the datatype [xsd:string](https://www.w3.org/TR/xmlschema11-2/#string).
    #[inline]
    #[deprecated(note = "Plain literal concept is removed in RDF 1.1", since = "0.3.0")]
    pub const fn is_plain(self) -> bool {
        matches!(
            self.0,
            LiteralRefContent::String(_) | LiteralRefContent::LanguageTaggedString { .. }
        )
    }

    #[inline]
    pub fn into_owned(self) -> Literal {
        Literal(match self.0 {
            LiteralRefContent::String(value) => LiteralContent::String(value.to_owned()),
            LiteralRefContent::LanguageTaggedString { value, language } => {
                LiteralContent::LanguageTaggedString {
                    value: value.to_owned(),
                    language: language.to_owned(),
                }
            }
            #[cfg(feature = "rdf-12")]
            LiteralRefContent::DirectionalLanguageTaggedString {
                value,
                language,
                direction,
            } => LiteralContent::DirectionalLanguageTaggedString {
                value: value.to_owned(),
                language: language.to_owned(),
                direction,
            },
            LiteralRefContent::TypedLiteral { value, datatype } => LiteralContent::TypedLiteral {
                value: value.to_owned(),
                datatype: datatype.into_owned(),
            },
        })
    }

    /// Extract components from this literal
    #[cfg(not(feature = "rdf-12"))]
    #[inline]
    #[deprecated(
        note = "Use directly .value(), .datatype() and .language()",
        since = "0.3.0"
    )]
    pub const fn destruct(self) -> (&'a str, Option<NamedNodeRef<'a>>, Option<&'a str>) {
        match self.0 {
            LiteralRefContent::String(s) => (s, None, None),
            LiteralRefContent::LanguageTaggedString { value, language } => {
                (value, None, Some(language))
            }
            LiteralRefContent::TypedLiteral { value, datatype } => (value, Some(datatype), None),
        }
    }
}

impl<'a> LiteralRef<'a> {
    /// Compares the exact RDF display representations without allocating formatted keys.
    ///
    /// This has the same order as comparing `self.to_string()` and `other.to_string()`.
    #[inline]
    pub fn cmp_display(self, other: Self) -> std::cmp::Ordering {
        self.display_characters().cmp(other.display_characters())
    }

    #[inline]
    fn display_parts(self) -> (&'a str, &'static str, &'a str, &'static str) {
        match self.0 {
            LiteralRefContent::String(value) => (value, "", "", ""),
            LiteralRefContent::LanguageTaggedString { value, language } => {
                (value, "@", language, "")
            }
            #[cfg(feature = "rdf-12")]
            LiteralRefContent::DirectionalLanguageTaggedString {
                value,
                language,
                direction,
            } => (
                value,
                "@",
                language,
                match direction {
                    BaseDirection::Ltr => "--ltr",
                    BaseDirection::Rtl => "--rtl",
                },
            ),
            LiteralRefContent::TypedLiteral { value, datatype } => {
                (value, "^^<", datatype.as_str(), ">")
            }
        }
    }

    #[inline]
    fn display_characters(self) -> impl Iterator<Item = char> + 'a {
        let (value, prefix, suffix, ending) = self.display_parts();
        std::iter::once('"')
            .chain(value.chars().flat_map(escaped_characters))
            .chain(std::iter::once('"'))
            .chain(prefix.chars())
            .chain(suffix.chars())
            .chain(ending.chars())
    }
}

impl fmt::Display for LiteralRef<'_> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (value, prefix, suffix, ending) = self.display_parts();
        print_quoted_str(value, f)?;
        if !prefix.is_empty() {
            f.write_str(prefix)?;
            f.write_str(suffix)?;
            f.write_str(ending)?;
        }
        Ok(())
    }
}

impl<'a> From<&'a Literal> for LiteralRef<'a> {
    #[inline]
    fn from(node: &'a Literal) -> Self {
        node.as_ref()
    }
}

impl<'a> From<LiteralRef<'a>> for Literal {
    #[inline]
    fn from(node: LiteralRef<'a>) -> Self {
        node.into_owned()
    }
}

impl<'a> From<&'a str> for LiteralRef<'a> {
    #[inline]
    fn from(value: &'a str) -> Self {
        LiteralRef(LiteralRefContent::String(value))
    }
}

impl PartialEq<Literal> for LiteralRef<'_> {
    #[inline]
    fn eq(&self, other: &Literal) -> bool {
        *self == other.as_ref()
    }
}

impl PartialEq<LiteralRef<'_>> for Literal {
    #[inline]
    fn eq(&self, other: &LiteralRef<'_>) -> bool {
        self.as_ref() == *other
    }
}

#[inline]
pub fn print_quoted_str(string: &str, f: &mut impl Write) -> fmt::Result {
    f.write_char('"')?;
    for c in string.chars() {
        match quoted_character_escape(c) {
            QuotedCharacter::Plain(c) => f.write_char(c)?,
            QuotedCharacter::Short(escape) => f.write_str(escape)?,
            QuotedCharacter::Unicode(digits) => {
                f.write_str("\\u")?;
                for digit in digits {
                    f.write_char(digit)?;
                }
            }
        }
    }
    f.write_char('"')
}

enum QuotedCharacter {
    Plain(char),
    Short(&'static str),
    Unicode([char; 4]),
}

#[inline]
fn quoted_character_escape(c: char) -> QuotedCharacter {
    match c {
        '\u{8}' => QuotedCharacter::Short("\\b"),
        '\t' => QuotedCharacter::Short("\\t"),
        '\n' => QuotedCharacter::Short("\\n"),
        '\u{C}' => QuotedCharacter::Short("\\f"),
        '\r' => QuotedCharacter::Short("\\r"),
        '"' => QuotedCharacter::Short("\\\""),
        '\\' => QuotedCharacter::Short("\\\\"),
        '\0'..='\u{1F}' | '\u{7F}' | '\u{FFFE}' | '\u{FFFF}' => {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            let codepoint = c as usize;
            QuotedCharacter::Unicode([
                char::from(HEX[(codepoint >> 12) & 15]),
                char::from(HEX[(codepoint >> 8) & 15]),
                char::from(HEX[(codepoint >> 4) & 15]),
                char::from(HEX[codepoint & 15]),
            ])
        }
        _ => QuotedCharacter::Plain(c),
    }
}

#[inline]
fn escaped_characters(c: char) -> impl Iterator<Item = char> {
    // bounded: one RDF escape has at most six characters.
    let mut buffer = ['\0'; 6];
    let length = match quoted_character_escape(c) {
        QuotedCharacter::Plain(c) => {
            buffer[0] = c;
            1
        }
        QuotedCharacter::Short(escape) => {
            let bytes = escape.as_bytes();
            buffer[0] = char::from(bytes[0]);
            buffer[1] = char::from(bytes[1]);
            2
        }
        QuotedCharacter::Unicode(digits) => {
            buffer = ['\\', 'u', digits[0], digits[1], digits[2], digits[3]];
            6
        }
    };
    buffer.into_iter().take(length)
}

/// A [directional language-tagged string](https://www.w3.org/TR/rdf12-concepts/#dfn-dir-lang-string) [base-direction](https://www.w3.org/TR/rdf12-concepts/#dfn-base-direction)
#[cfg(feature = "rdf-12")]
#[derive(Eq, PartialEq, Debug, Clone, Copy, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum BaseDirection {
    /// the initial text direction is set to left-to-right
    #[cfg_attr(feature = "serde", serde(rename = "ltr"))]
    Ltr,
    /// the initial text direction is set to right-to-left
    #[cfg_attr(feature = "serde", serde(rename = "rtl"))]
    Rtl,
}

#[cfg(feature = "rdf-12")]
impl fmt::Display for BaseDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ltr => "ltr",
            Self::Rtl => "rtl",
        })
    }
}

#[cfg(feature = "serde")]
impl Serialize for Literal {
    #[inline]
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.as_ref().serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl Serialize for LiteralRef<'_> {
    #[inline]
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[expect(clippy::struct_field_names)]
        #[derive(Serialize)]
        #[serde(rename = "Literal")]
        struct Value<'a> {
            value: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            language: Option<&'a str>,
            #[cfg(feature = "rdf-12")]
            #[serde(skip_serializing_if = "Option::is_none")]
            direction: Option<BaseDirection>,
            #[serde(skip_serializing_if = "Option::is_none")]
            datatype: Option<&'a str>,
        }
        match self.0 {
            LiteralRefContent::String(value) => Value {
                value,
                language: None,
                #[cfg(feature = "rdf-12")]
                direction: None,
                datatype: None,
            },
            LiteralRefContent::LanguageTaggedString { value, language } => Value {
                value,
                language: Some(language),
                #[cfg(feature = "rdf-12")]
                direction: None,
                datatype: None,
            },
            #[cfg(feature = "rdf-12")]
            LiteralRefContent::DirectionalLanguageTaggedString {
                value,
                language,
                direction,
            } => Value {
                value,
                language: Some(language),
                direction: Some(direction),
                datatype: None,
            },
            LiteralRefContent::TypedLiteral { value, datatype } => Value {
                value,
                language: None,
                #[cfg(feature = "rdf-12")]
                direction: None,
                datatype: Some(datatype.as_str()),
            },
        }
        .serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for Literal {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[expect(clippy::struct_field_names)]
        #[derive(Deserialize)]
        #[serde(rename = "Literal")]
        struct Value {
            value: String,
            language: Option<String>,
            #[cfg(feature = "rdf-12")]
            direction: Option<BaseDirection>,
            datatype: Option<String>,
        }
        let Value {
            value,
            language,
            #[cfg(feature = "rdf-12")]
            direction,
            datatype,
        } = Value::deserialize(deserializer)?;
        if let Some(language) = language {
            #[cfg(feature = "rdf-12")]
            if let Some(direction) = direction {
                return Literal::new_directional_language_tagged_literal(
                    value, language, direction,
                )
                .map_err(de::Error::custom);
            }
            Literal::new_language_tagged_literal(value, language).map_err(de::Error::custom)
        } else if let Some(datatype) = datatype {
            Ok(Literal::new_typed_literal(
                value,
                NamedNode::new(datatype).map_err(de::Error::custom)?,
            ))
        } else {
            Ok(Literal::new_simple_literal(value))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(feature = "rdf-10-string-identity"))]
    fn test_simple_literal_equality() {
        assert_eq!(
            Literal::new_simple_literal("foo"),
            Literal::new_typed_literal("foo", xsd::STRING)
        );
        assert_eq!(
            Literal::new_simple_literal("foo"),
            LiteralRef::new_typed_literal("foo", xsd::STRING)
        );
        assert_eq!(
            LiteralRef::new_simple_literal("foo"),
            Literal::new_typed_literal("foo", xsd::STRING)
        );
        assert_eq!(
            LiteralRef::new_simple_literal("foo"),
            LiteralRef::new_typed_literal("foo", xsd::STRING)
        );
    }

    #[test]
    #[cfg(feature = "rdf-10-string-identity")]
    fn test_explicit_string_identity() {
        let plain = Literal::new_simple_literal("foo");
        let typed = Literal::new_typed_literal("foo", xsd::STRING);
        assert_ne!(plain, typed);
        assert_ne!(plain.as_ref(), typed.as_ref());
        assert_eq!(plain.datatype(), typed.datatype());
        assert!(!plain.has_explicit_datatype());
        assert!(typed.has_explicit_datatype());
        assert_eq!(typed, LiteralRef::new_typed_literal("foo", xsd::STRING));
        assert_eq!(plain.to_string(), "\"foo\"");
        assert_eq!(
            typed.to_string(),
            "\"foo\"^^<http://www.w3.org/2001/XMLSchema#string>"
        );
    }

    #[test]
    fn test_float_format() {
        assert_eq!("INF", Literal::from(f32::INFINITY).value());
        assert_eq!("INF", Literal::from(f64::INFINITY).value());
        assert_eq!("-INF", Literal::from(f32::NEG_INFINITY).value());
        assert_eq!("-INF", Literal::from(f64::NEG_INFINITY).value());
        assert_eq!("NaN", Literal::from(f32::NAN).value());
        assert_eq!("NaN", Literal::from(f64::NAN).value());
    }

    #[test]
    #[cfg(feature = "serde")]
    fn test_serde() {
        // Simple literal
        let simple = Literal::new_simple_literal("foo");
        let j = serde_json::to_string(&simple).unwrap();
        assert_eq!("{\"value\":\"foo\"}", j);
        let simple2: Literal = serde_json::from_str(&j).unwrap();
        assert_eq!(simple, simple2);

        #[cfg(feature = "rdf-10-string-identity")]
        {
            let explicit = Literal::new_typed_literal("foo", xsd::STRING);
            let json = serde_json::to_string(&explicit).unwrap();
            let restored: Literal = serde_json::from_str(&json).unwrap();
            assert_eq!(explicit, restored);
            assert_ne!(simple, restored);
        }

        // Typed literal
        let typed = Literal::new_typed_literal("foo", xsd::BOOLEAN);
        let j = serde_json::to_string(&typed).unwrap();
        assert_eq!(
            "{\"value\":\"foo\",\"datatype\":\"http://www.w3.org/2001/XMLSchema#boolean\"}",
            j
        );
        let typed2: Literal = serde_json::from_str(&j).unwrap();
        assert_eq!(typed, typed2);

        // Language-tagged string
        let lt = Literal::new_language_tagged_literal("foo", "en").unwrap();
        let j = serde_json::to_string(&lt).unwrap();
        assert_eq!("{\"value\":\"foo\",\"language\":\"en\"}", j);
        let lt2: Literal = serde_json::from_str(&j).unwrap();
        assert_eq!(lt, lt2);
    }
}
