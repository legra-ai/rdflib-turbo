use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::OnceLock;

use num_bigint::{BigInt, Sign};
use oxigraph::model::*;
use oxigraph::sparql::{PreparedSparqlQuery, SparqlEvaluator};
use regex_syntax::hir::{Class, ClassUnicode, HirKind};

use crate::graph::*;
use crate::path::rdf_list;
use crate::{Error, GraphRead, Result};

/// Compiler-local Unicode scalar policy; no Python objects or graph state.
#[derive(Clone, Copy)]
pub(crate) struct DecimalDigitPolicy(fn(char) -> Option<u8>);

impl DecimalDigitPolicy {
    pub(crate) fn new(non_ascii: fn(char) -> Option<u8>) -> Self {
        Self(non_ascii)
    }

    pub(crate) fn decimal(self, ch: char) -> Option<u8> {
        if ch.is_ascii_digit() {
            Some(ch as u8 - b'0')
        } else {
            (self.0)(ch)
        }
    }
}

impl Default for DecimalDigitPolicy {
    fn default() -> Self {
        Self(decimal_digit)
    }
}

#[path = "constraint_list.rs"]
mod membership_list;

#[cfg(test)]
#[path = "constraints_tests.rs"]
mod coverage_tests;

pub(crate) const PARAMETERS: &[&str] = &[
    "sparql",
    "minCount",
    "maxCount",
    "hasValue",
    "class",
    "datatype",
    "nodeKind",
    "minExclusive",
    "minInclusive",
    "maxExclusive",
    "maxInclusive",
    "minLength",
    "maxLength",
    "pattern",
    "flags",
    "languageIn",
    "uniqueLang",
    "singleLine",
    "equals",
    "disjoint",
    "lessThan",
    "lessThanOrEquals",
    "in",
    "closed",
    "ignoredProperties",
    "node",
    "not",
    "and",
    "or",
    "xone",
    "qualifiedValueShape",
    "qualifiedMinCount",
    "qualifiedMaxCount",
    "qualifiedValueShapesDisjoint",
];

pub(crate) enum Constraint {
    NodeExpression {
        expression: crate::expression::NodeExpression,
        node: Term,
        messages: Vec<Term>,
    },
    Sparql(Box<crate::sparql::SparqlConstraint>),
    Count {
        minimum: bool,
        count: IntegerParameter,
    },
    HasValue(Term),
    Class(Term),
    Datatype(NamedNode),
    NodeKind(String),
    Pattern(crate::pattern::Pattern),
    Expression {
        component: &'static str,
        query: Box<PreparedSparqlQuery>,
    },
    Length {
        minimum: bool,
        count: IntegerParameter,
    },
    LanguageIn(Vec<String>),
    UniqueLang,
    SingleLine,
    In(HashSet<Term>),
    Pair {
        kind: &'static str,
        predicate: NamedNode,
        query: Option<Box<PreparedSparqlQuery>>,
    },
    Logical {
        kind: &'static str,
        shapes: Vec<usize>,
    },
    Node(usize),
    Qualified {
        shape: usize,
        minimum: Option<IntegerParameter>,
        maximum: Option<IntegerParameter>,
        disjoint: bool,
        siblings: Vec<usize>,
    },
    Closed(HashSet<NamedNode>),
}

pub(crate) fn named(term: Term, context: &str) -> Result<NamedNode> {
    match term {
        Term::NamedNode(n) => Ok(n),
        _ => Err(Error::Invalid(format!("{context} requires an IRI"))),
    }
}

/// Keep the exact declaration alongside its arbitrary-precision comparison value.
pub(crate) struct IntegerParameter {
    pub literal: Literal,
    pub number: BigInt,
}

impl IntegerParameter {
    pub fn allows(&self, size: usize, minimum: bool) -> bool {
        let size = BigInt::from(size);
        if minimum {
            size >= self.number
        } else {
            size <= self.number
        }
    }

    pub fn is_zero(&self) -> bool {
        self.number.sign() == Sign::NoSign
    }
}

impl fmt::Display for IntegerParameter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Python bool is an int subclass in qualified-count declarations.
        if self.literal.datatype() == iri(XSD, "boolean").as_ref() {
            f.write_str(if self.is_zero() { "False" } else { "True" })
        } else {
            self.number.fmt(f)
        }
    }
}

#[cfg(test)]
fn integer(term: Term, name: &str) -> Result<IntegerParameter> {
    integer_with_policy(term, name, Default::default())
}

fn integer_with_policy(
    term: Term,
    name: &str,
    digits: DecimalDigitPolicy,
) -> Result<IntegerParameter> {
    let invalid = || Error::Invalid(format!("Invalid integer parameter sh:{name}: {term}"));
    let Term::Literal(literal) = &term else {
        return Err(invalid());
    };
    let kind = literal.datatype().as_str().strip_prefix(XSD);
    let qualified = matches!(name, "qualifiedMinCount" | "qualifiedMaxCount");
    let length = matches!(name, "minLength" | "maxLength");
    let number = if qualified && kind == Some("boolean") {
        match literal.value() {
            "true" | "1" => BigInt::from(1),
            "false" | "0" => BigInt::from(0),
            _ => return Err(invalid()),
        }
    } else {
        let integer_kind = matches!(
            kind,
            Some(
                "integer"
                    | "int"
                    | "long"
                    | "short"
                    | "byte"
                    | "unsignedLong"
                    | "unsignedInt"
                    | "unsignedShort"
                    | "unsignedByte"
                    | "nonNegativeInteger"
                    | "positiveInteger"
                    | "nonPositiveInteger"
                    | "negativeInteger"
            )
        );
        if !integer_kind
            || (!qualified && !length && kind != Some("integer"))
            // The pinned reference's length whitelist excludes xsd:byte as
            // well as the negative/nonpositive datatypes, even for zero.
            || (length && matches!(kind, Some("byte" | "negativeInteger" | "nonPositiveInteger")))
        {
            return Err(invalid());
        }
        integer_lexical_with_policy(literal.value(), digits)
            .ok_or_else(|| integer_conversion_error(name))?
    };
    if !qualified && number.sign() == Sign::Minus {
        return Err(invalid());
    }
    Ok(IntegerParameter {
        literal: literal.clone(),
        number,
    })
}

fn integer_conversion_error(name: &str) -> Error {
    let (message, link) = match name {
        "minLength" => (
            "sh:minLength value must be a literal value with an integer.",
            "https://www.w3.org/TR/shacl/#MinLengthConstraintComponent",
        ),
        "maxLength" => (
            "sh:maxLength value must be a literal value with an integer.",
            "https://www.w3.org/TR/shacl/#MaxLengthConstraintComponent",
        ),
        "qualifiedMinCount" => (
            "QualifiedMinCountConstraintComponent sh:qualifiedMinCount must be a Literal with Int.",
            "https://www.w3.org/TR/shacl/#QualifiedValueShapeConstraintComponent",
        ),
        "qualifiedMaxCount" => (
            "QualifiedMaxCountConstraintComponent sh:qualifiedMaxCount must be a Literal with Int.",
            "https://www.w3.org/TR/shacl/#QualifiedValueShapeConstraintComponent",
        ),
        "minCount" | "maxCount" => {
            return Error::TypeError(
                "int() argument must be a string, a bytes-like object or a real number, not 'NoneType'",
            );
        }
        _ => unreachable!("only integer constraint parameters use this parser"),
    };
    Error::ConstraintLoad { message, link }
}

fn integer_lexical_with_policy(text: &str, digits: DecimalDigitPolicy) -> Option<BigInt> {
    // RDFLib uses Python int for these literals: decimal digits may be Unicode,
    // with one underscore between digits. BigInt's own parser is more lenient
    // about underscores, so validate and normalize before parsing its value.
    let mut chars = text.trim().chars().peekable();
    let mut normalized = String::with_capacity(text.len());
    if chars.peek().is_some_and(|ch| matches!(ch, '+' | '-')) {
        normalized.push(chars.next()?);
    }
    let mut after_digit = false;
    for ch in chars {
        if ch == '_' {
            if !after_digit {
                return None;
            }
            after_digit = false;
        } else {
            normalized.push(char::from(b'0' + digits.decimal(ch)?));
            after_digit = true;
        }
    }
    if !after_digit {
        return None;
    }
    normalized.parse().ok()
}

pub(crate) fn decimal_digit(ch: char) -> Option<u8> {
    if ch.is_ascii_digit() {
        return Some(ch as u8 - b'0');
    }
    static DIGITS: OnceLock<ClassUnicode> = OnceLock::new();
    let digits = DIGITS.get_or_init(|| {
        let hir = regex_syntax::Parser::new()
            .parse(r"\p{Nd}")
            .expect("Unicode decimal-digit class is valid");
        let HirKind::Class(Class::Unicode(class)) = hir.kind() else {
            unreachable!("decimal digits form a Unicode class");
        };
        class.clone()
    });
    let ranges = digits.ranges();
    let range = ranges.get(ranges.partition_point(|range| range.end() < ch))?;
    // Unicode Nd digits occur in consecutive runs of 0 through 9; adjacent
    // digit alphabets may share a range, hence the modulo operation.
    (range.start() <= ch).then(|| ((ch as u32 - range.start() as u32) % 10) as u8)
}

fn string(term: Term, name: &str) -> Result<String> {
    if let Term::Literal(v) = term
        && v.datatype() == iri(XSD, "string").as_ref()
    {
        return Ok(v.value().to_owned());
    }
    Err(Error::Invalid(format!("sh:{name} requires an xsd:string")))
}

fn pattern_text(term: Term) -> Result<String> {
    // Core patterns also accept ordinary language-tagged text. Other string
    // parameter roles retain their existing independent contracts.
    match term {
        Term::Literal(value) if value.language().is_some() => Ok(value.value().to_owned()),
        value => string(value, "pattern"),
    }
}

fn expression(text: &str) -> Result<Box<PreparedSparqlQuery>> {
    Ok(Box::new(
        SparqlEvaluator::new()
            .parse_query(&format!("SELECT ?value ?other ({text} AS ?ok) WHERE {{}}"))
            .map_err(|e| Error::Invalid(format!("Constraint expression: {e}")))?,
    ))
}

pub(crate) fn shape_references(
    store: &(impl GraphRead + ?Sized),
    advanced: bool,
) -> Result<HashSet<Term>> {
    let mut result = HashSet::new();
    for name in [
        "property",
        "condition",
        "node",
        "not",
        "qualifiedValueShape",
        "and",
        "or",
        "xone",
    ] {
        if name == "condition" && !advanced {
            continue;
        }
        for node in subjects(store, &iri(SH, name), None)? {
            for target in values(store, &node, name)? {
                if ["and", "or", "xone"].contains(&name)
                    || (name == "condition"
                        && (target == Term::from(iri(RDF, "nil"))
                            || !objects(store, &target, &iri(RDF, "first"))?.is_empty()))
                {
                    result.extend(rdf_list(store, &target)?);
                } else {
                    result.insert(target);
                }
            }
        }
    }
    Ok(result)
}

/// Read Core parameter values without compiling queries or resolving shape indexes.
pub(crate) fn read(
    store: &(impl GraphRead + ?Sized),
    node: &Term,
    property: bool,
    digits: DecimalDigitPolicy,
) -> Result<Vec<crate::model::Component<Term>>> {
    use crate::model::{Component as C, NodeKind, ShaclPath};
    use crate::structural::named_role;
    let mut result = Vec::new();
    for name in ["minCount", "maxCount"] {
        if let Some(value) = one(store, node, name)? {
            if !property {
                return Err(Error::Invalid(format!(
                    "sh:{name} requires a property shape"
                )));
            }
            result.push(if name == "minCount" {
                C::MinCount(value)
            } else {
                C::MaxCount(value)
            });
        }
    }
    result.extend(
        values(store, node, "hasValue")?
            .into_iter()
            .map(C::HasValue),
    );
    for value in values(store, node, "class")? {
        result.push(C::Class(named_role(value, "sh:class")?));
    }
    if let Some(value) = one(store, node, "datatype")? {
        result.push(C::Datatype(named_role(value, "sh:datatype")?));
    }
    if let Some(value) = one(store, node, "nodeKind")? {
        let kind = named(value, "sh:nodeKind")?;
        result.push(C::NodeKind(NodeKind::from_iri(kind.as_str()).map_err(
            |_| Error::Invalid(format!("Invalid sh:nodeKind {kind}")),
        )?));
    }
    for name in [
        "minExclusive",
        "minInclusive",
        "maxExclusive",
        "maxInclusive",
    ] {
        if let Some(value) = one(store, node, name)? {
            if !matches!(value, Term::Literal(_)) {
                return Err(Error::Invalid(format!("sh:{name} requires a literal")));
            }
            result.push(match name {
                "minExclusive" => C::MinExclusive(value),
                "minInclusive" => C::MinInclusive(value),
                "maxExclusive" => C::MaxExclusive(value),
                "maxInclusive" => C::MaxInclusive(value),
                _ => unreachable!(),
            });
        }
    }
    for name in ["minLength", "maxLength"] {
        if let Some(value) = one(store, node, name)? {
            result.push(if name == "minLength" {
                C::MinLength(value)
            } else {
                C::MaxLength(value)
            });
        }
    }
    let flags = one(store, node, "flags")?;
    if let Some(value) = &flags {
        string(value.clone(), "flags")?;
    }
    for pattern in values(store, node, "pattern")? {
        let Term::Literal(pattern) = pattern else {
            return Err(Error::Invalid("sh:pattern requires a literal".into()));
        };
        result.push(C::Pattern {
            pattern: Term::Literal(pattern),
            flags: flags.clone(),
        });
    }
    if let Some(head) = one(store, node, "languageIn")? {
        result.push(C::LanguageIn(
            rdf_list(store, &head)?
                .into_iter()
                .map(|v| string(v, "languageIn"))
                .collect::<Result<_>>()?,
        ));
    }
    if boolean(store, node, "uniqueLang")? {
        if !property {
            return Err(Error::Invalid(
                "sh:uniqueLang requires a property shape".into(),
            ));
        }
        result.push(C::UniqueLang(true));
    }
    if one(store, node, "singleLine")?.is_some() {
        result.push(C::SingleLine(boolean(store, node, "singleLine")?));
    }
    if let Some(head) = one(store, node, "in")? {
        result.push(C::In(membership_list::read(store, &head, digits)?));
    }
    for name in ["equals", "disjoint", "lessThan", "lessThanOrEquals"] {
        for value in values(store, node, name)? {
            if ["lessThan", "lessThanOrEquals"].contains(&name) && !property {
                return Err(Error::Invalid(format!(
                    "sh:{name} requires a property shape"
                )));
            }
            let path = ShaclPath::Predicate(named_role(value, name)?);
            result.push(match name {
                "equals" => C::Equals(path),
                "disjoint" => C::Disjoint(path),
                "lessThan" => C::LessThan(path),
                "lessThanOrEquals" => C::LessThanOrEquals(path),
                _ => unreachable!(),
            });
        }
    }
    result.extend(values(store, node, "node")?.into_iter().map(C::Node));
    for name in ["not", "and", "or", "xone"] {
        for value in values(store, node, name)? {
            result.push(match name {
                "not" => C::Not(value),
                "and" => C::And(rdf_list(store, &value)?),
                "or" => C::Or(rdf_list(store, &value)?),
                "xone" => C::Xone(rdf_list(store, &value)?),
                _ => unreachable!(),
            });
        }
    }
    let minimum = one(store, node, "qualifiedMinCount")?;
    let maximum = one(store, node, "qualifiedMaxCount")?;
    let disjoint = boolean(store, node, "qualifiedValueShapesDisjoint")?;
    if let Some(value) = one(store, node, "qualifiedValueShape")? {
        if !property {
            return Err(Error::Invalid(
                "sh:qualifiedValueShape requires a property shape".into(),
            ));
        }
        result.push(C::QualifiedValueShape {
            shape: value,
            min_count: minimum,
            max_count: maximum,
            disjoint,
        });
    } else {
        // These parameters do not activate a constraint by themselves, but
        // malformed integer declarations still fail under the existing contract.
        for (value, name) in [
            (minimum, "qualifiedMinCount"),
            (maximum, "qualifiedMaxCount"),
        ] {
            if let Some(value) = value {
                integer_with_policy(value, name, digits)?;
            }
        }
    }
    let ignored = one(store, node, "ignoredProperties")?;
    let closed = one(store, node, "closed")?;
    if ignored.is_some() && closed.is_none() {
        return Err(Error::Invalid(
            "sh:ignoredProperties requires sh:closed".into(),
        ));
    }
    let ignored_properties = ignored
        .map(|head| rdf_list(store, &head))
        .transpose()?
        .unwrap_or_default()
        .into_iter()
        .map(|v| named_role(v, "sh:ignoredProperties"))
        .collect::<Result<_>>()?;
    let is_closed = boolean(store, node, "closed")?;
    if closed.is_some() {
        result.push(C::Closed {
            is_closed,
            ignored_properties,
        });
    }
    Ok(result)
}

/// Lower an owned Core declaration; source lookups are only for cross-shape relationships.
pub(crate) fn lower_one(
    store: &(impl GraphRead + ?Sized),
    node: &Term,
    property: bool,
    indices: &HashMap<Term, usize>,
    declaration: crate::model::Component<Term>,
    digits: DecimalDigitPolicy,
    regex: Option<(
        &std::sync::Arc<crate::RegexPolicy>,
        &dyn crate::RegexLexicalResolver,
    )>,
) -> Result<Option<Constraint>> {
    use crate::model::{Component as C, ShaclPath};
    let reference = |n: &Term| {
        indices
            .get(n)
            .copied()
            .ok_or_else(|| Error::Invalid(format!("Unknown shape {n}")))
    };
    let range = |value: Term, component, operator| -> Result<Constraint> {
        Ok(Constraint::Expression {
            component,
            query: expression(&format!("?value {operator} {value}"))?,
        })
    };
    let pair = |path: ShaclPath, kind, operator: Option<&str>| -> Result<Constraint> {
        let ShaclPath::Predicate(predicate) = path else {
            return Err(Error::Invalid(
                "Pair constraints require a predicate IRI".into(),
            ));
        };
        if operator.is_some() && !property {
            return Err(Error::Invalid(
                "Comparison requires a property shape".into(),
            ));
        }
        Ok(Constraint::Pair {
            kind,
            predicate,
            query: operator
                .map(|op| expression(&format!("?value {op} ?other")))
                .transpose()?,
        })
    };
    Ok(Some(match declaration {
        C::MinCount(value) => Constraint::Count {
            minimum: true,
            count: integer_with_policy(value, "minCount", digits)?,
        },
        C::MaxCount(value) => Constraint::Count {
            minimum: false,
            count: integer_with_policy(value, "maxCount", digits)?,
        },
        C::MinLength(value) => Constraint::Length {
            minimum: true,
            count: integer_with_policy(value, "minLength", digits)?,
        },
        C::MaxLength(value) => Constraint::Length {
            minimum: false,
            count: integer_with_policy(value, "maxLength", digits)?,
        },
        C::HasValue(value) => Constraint::HasValue(value),
        C::Class(value) => Constraint::Class(value.into()),
        C::Datatype(value) => Constraint::Datatype(value),
        C::NodeKind(kind) => Constraint::NodeKind(
            kind.as_iri()
                .strip_prefix(SH)
                .expect("SHACL node kind IRI")
                .to_owned(),
        ),
        C::MinExclusive(value) => range(value, "MinExclusiveConstraintComponent", ">")?,
        C::MinInclusive(value) => range(value, "MinInclusiveConstraintComponent", ">=")?,
        C::MaxExclusive(value) => range(value, "MaxExclusiveConstraintComponent", "<")?,
        C::MaxInclusive(value) => range(value, "MaxInclusiveConstraintComponent", "<=")?,
        C::Pattern { pattern, flags } => {
            let pattern = pattern_text(pattern)?;
            let flags = flags
                .map(|value| string(value, "flags"))
                .transpose()?
                .unwrap_or_default();
            Constraint::Pattern(match regex {
                Some((policy, lexical)) => crate::pattern::Pattern::compile_host(
                    &pattern,
                    &flags,
                    policy.clone(),
                    lexical,
                )?,
                None => crate::pattern::Pattern::compile(&pattern, &flags)?,
            })
        }
        C::LanguageIn(values) => {
            Constraint::LanguageIn(values.into_iter().map(|v| v.to_ascii_lowercase()).collect())
        }
        C::UniqueLang(true) => Constraint::UniqueLang,
        C::UniqueLang(false) => return Ok(None),
        C::SingleLine(true) => Constraint::SingleLine,
        C::SingleLine(false) => return Ok(None),
        C::In(values) => Constraint::In(values.into_iter().collect()),
        C::Equals(path) => pair(path, "EqualsConstraintComponent", None)?,
        C::Disjoint(path) => pair(path, "DisjointConstraintComponent", None)?,
        C::LessThan(path) => pair(path, "LessThanConstraintComponent", Some("<"))?,
        C::LessThanOrEquals(path) => pair(path, "LessThanOrEqualsConstraintComponent", Some("<="))?,
        C::Node(value) => Constraint::Node(reference(&value)?),
        C::Not(value) => Constraint::Logical {
            kind: "NotConstraintComponent",
            shapes: vec![reference(&value)?],
        },
        C::And(values) => Constraint::Logical {
            kind: "AndConstraintComponent",
            shapes: values.iter().map(reference).collect::<Result<_>>()?,
        },
        C::Or(values) => Constraint::Logical {
            kind: "OrConstraintComponent",
            shapes: values.iter().map(reference).collect::<Result<_>>()?,
        },
        C::Xone(values) => Constraint::Logical {
            kind: "XoneConstraintComponent",
            shapes: values.iter().map(reference).collect::<Result<_>>()?,
        },
        C::QualifiedValueShape {
            shape,
            min_count,
            max_count,
            disjoint,
        } => {
            let mut siblings = HashSet::new();
            if disjoint {
                for parent in subjects(store, &iri(SH, "property"), Some(node))? {
                    for sibling in values(store, &parent, "property")? {
                        for qualified in values(store, &sibling, "qualifiedValueShape")? {
                            if qualified != shape {
                                siblings.insert(reference(&qualified)?);
                            }
                        }
                    }
                }
            }
            Constraint::Qualified {
                shape: reference(&shape)?,
                minimum: min_count
                    .map(|v| integer_with_policy(v, "qualifiedMinCount", digits))
                    .transpose()?,
                maximum: max_count
                    .map(|v| integer_with_policy(v, "qualifiedMaxCount", digits))
                    .transpose()?,
                disjoint,
                siblings: siblings.into_iter().collect(),
            }
        }
        C::Closed {
            is_closed,
            ignored_properties,
        } => {
            if !is_closed {
                return Ok(None);
            }
            let mut allowed: HashSet<_> = ignored_properties.into_iter().collect();
            for property in values(store, node, "property")? {
                if let Some(Term::NamedNode(path)) = one(store, &property, "path")? {
                    allowed.insert(path);
                }
            }
            Constraint::Closed(allowed)
        }
        C::Sparql(_) | C::Expression(_) | C::Custom(_) => {
            return Err(Error::Invalid(
                "Advanced declaration requires the SPARQL/AF lowering context".into(),
            ));
        }
    }))
}

#[cfg(test)]
mod integer_tests {
    use super::*;

    #[test]
    fn invalid_integer_parameters_return_errors_without_panicking() {
        for lexical in ["no", "²", "١٢x", "1.0"] {
            let term = Literal::new_typed_literal(lexical, iri(XSD, "integer"));
            assert!(integer(term.into(), "minCount").is_err());
        }
    }
}
