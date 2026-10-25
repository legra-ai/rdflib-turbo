//! Lower SPARQL regex builtins to native Python-compatible scalar functions.
//! Lowering the parsed algebra keeps query strings, IRIs, and comments intact.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use fancy_regex::Regex;
use oxigraph::model::{Literal, NamedNode, Term, vocab::xsd};
use oxigraph::sparql::SparqlEvaluator;
use spargebra::Query;
use spargebra::algebra::{
    AggregateExpression, Expression, Function, GraphPattern, OrderExpression,
};

use crate::functions::FunctionErrors;
use crate::pattern::Pattern;
use crate::{Error, Result};

#[cfg(test)]
#[path = "sparql_regex_tests.rs"]
mod tests;

const REGEX: &str = "urn:pyshacl-turbo:internal:regex";
const REPLACE: &str = "urn:pyshacl-turbo:internal:replace";
const CACHE_LIMIT: usize = 128;

type PatternCache = Arc<Mutex<HashMap<(String, String), Arc<Pattern>>>>;

#[derive(Clone, Default)]
pub(crate) struct RegexFunctions {
    required: bool,
    cache: PatternCache,
}

impl RegexFunctions {
    pub fn lower(query: &mut Query) -> Self {
        let mut functions = Self::default();
        let (Query::Select { pattern, .. }
        | Query::Ask { pattern, .. }
        | Query::Construct { pattern, .. }
        | Query::Describe { pattern, .. }) = query;
        functions.pattern(pattern);
        functions
    }

    pub fn required(&self) -> bool {
        self.required
    }

    pub fn register(
        &self,
        mut evaluator: SparqlEvaluator,
        errors: FunctionErrors,
    ) -> SparqlEvaluator {
        if !self.required {
            return evaluator;
        }
        for (name, replace) in [(REGEX, false), (REPLACE, true)] {
            let functions = self.clone();
            let errors = errors.clone();
            evaluator =
                evaluator.with_custom_function(NamedNode::new_unchecked(name), move |args| {
                    if errors
                        .lock()
                        .expect("function error mutex poisoned")
                        .is_some()
                    {
                        return None;
                    }
                    match functions.call(args, replace) {
                        Ok(value) => value,
                        Err(error) => {
                            *errors.lock().expect("function error mutex poisoned") = Some(error);
                            None
                        }
                    }
                });
        }
        evaluator
    }

    fn compiled(&self, pattern: &str, flags: &str) -> Result<Arc<Pattern>> {
        let key = (pattern.to_owned(), flags.to_owned());
        if let Some(pattern) = self
            .cache
            .lock()
            .expect("pattern cache poisoned")
            .get(&key)
            .cloned()
        {
            return Ok(pattern);
        }
        let pattern = Arc::new(Pattern::compile_sparql(pattern, flags)?);
        let mut cache = self.cache.lock().expect("pattern cache poisoned");
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(key, pattern.clone());
        Ok(pattern)
    }

    fn call(&self, args: &[Term], replace: bool) -> Result<Option<Term>> {
        let arity = if replace { 3 } else { 2 };
        if !(arity..=arity + 1).contains(&args.len()) {
            return Ok(None);
        }
        let (Some(text), Some(pattern)) = (string(&args[0]), string(&args[1])) else {
            return Ok(None);
        };
        let flags = args.get(arity).map(flag_text).unwrap_or_default();
        if replace {
            let Some(replacement) = string(&args[2]) else {
                return Ok(None);
            };
            // RDFLib 7.6 passes its i/m/s mask as re.sub's positional count,
            // not flags. Preserve the pinned behavior, including inline flags.
            let count = (usize::from(flags.contains('i')) * 2)
                | (usize::from(flags.contains('m')) * 8)
                | (usize::from(flags.contains('s')) * 16);
            let compiled = self.compiled(pattern.value(), "")?;
            let value = compiled.replace(text.value(), replacement.value(), count)?;
            let result = if let Some(language) = text.language() {
                Literal::new_language_tagged_literal_unchecked(value, language)
            } else if text.has_explicit_datatype() {
                Literal::new_typed_literal(value, text.datatype().into_owned())
            } else {
                Literal::new_simple_literal(value)
            };
            Ok(Some(result.into()))
        } else {
            let compiled = self.compiled(pattern.value(), flags)?;
            let matched = compiled
                .is_match(text.value())
                .map_err(|e| Error::Runtime(e.to_string().replace("sh:pattern", "SPARQL REGEX")))?;
            Ok(Some(Literal::from(matched).into()))
        }
    }

    fn expression(&mut self, expression: &mut Expression) {
        use Expression as E;
        match expression {
            E::Or(a, b)
            | E::And(a, b)
            | E::Equal(a, b)
            | E::SameTerm(a, b)
            | E::Greater(a, b)
            | E::GreaterOrEqual(a, b)
            | E::Less(a, b)
            | E::LessOrEqual(a, b)
            | E::Add(a, b)
            | E::Subtract(a, b)
            | E::Multiply(a, b)
            | E::Divide(a, b) => {
                self.expression(a);
                self.expression(b);
            }
            E::UnaryPlus(a) | E::UnaryMinus(a) | E::Not(a) => self.expression(a),
            E::In(a, args) => {
                self.expression(a);
                for arg in args {
                    self.expression(arg);
                }
            }
            E::If(a, b, c) => {
                self.expression(a);
                self.expression(b);
                self.expression(c);
            }
            E::Coalesce(args) => {
                for arg in args {
                    self.expression(arg);
                }
            }
            E::FunctionCall(function, args) => {
                for arg in args {
                    self.expression(arg);
                }
                let name = match function {
                    Function::Regex => REGEX,
                    Function::Replace => REPLACE,
                    _ => return,
                };
                *function = Function::Custom(NamedNode::new_unchecked(name));
                self.required = true;
            }
            E::Exists(pattern) => self.pattern(pattern),
            E::NamedNode(_) | E::Literal(_) | E::Variable(_) | E::Bound(_) => (),
        }
    }

    fn pattern(&mut self, pattern: &mut GraphPattern) {
        use GraphPattern as G;
        match pattern {
            G::Bgp { .. } | G::Path { .. } | G::Values { .. } => (),
            G::Join { left, right }
            | G::Union { left, right }
            | G::Minus { left, right }
            | G::Lateral { left, right } => {
                self.pattern(left);
                self.pattern(right);
            }
            G::LeftJoin {
                left,
                right,
                expression,
            } => {
                self.pattern(left);
                self.pattern(right);
                if let Some(expression) = expression {
                    self.expression(expression);
                }
            }
            G::Filter { inner, expr } => {
                self.pattern(inner);
                self.expression(expr);
            }
            G::Extend {
                inner, expression, ..
            } => {
                self.pattern(inner);
                self.expression(expression);
            }
            G::OrderBy { inner, expression } => {
                self.pattern(inner);
                for order in expression {
                    let (OrderExpression::Asc(expr) | OrderExpression::Desc(expr)) = order;
                    self.expression(expr);
                }
            }
            G::Group {
                inner, aggregates, ..
            } => {
                self.pattern(inner);
                for (_, aggregate) in aggregates {
                    if let AggregateExpression::FunctionCall { expr, .. } = aggregate {
                        self.expression(expr);
                    }
                }
            }
            G::Graph { inner, .. }
            | G::Project { inner, .. }
            | G::ProjectWithSourceScope { inner, .. }
            | G::Distinct { inner }
            | G::Reduced { inner }
            | G::Slice { inner, .. }
            | G::Service { inner, .. } => self.pattern(inner),
        }
    }
}

fn string(term: &Term) -> Option<&Literal> {
    let Term::Literal(value) = term else {
        return None;
    };
    (value.language().is_some()
        || value.datatype().as_str() == "http://www.w3.org/2001/XMLSchema#string")
        .then_some(value)
}

fn flag_text(term: &Term) -> &str {
    match term {
        // RDFLib tests the flag term's truth value before iterating its text.
        // Boolean false must not accidentally enable 's' from its spelling.
        Term::Literal(value)
            if value.datatype() == xsd::BOOLEAN && matches!(value.value(), "false" | "0") =>
        {
            ""
        }
        Term::Literal(value) => value.value(),
        Term::NamedNode(value) => value.as_str(),
        Term::BlankNode(value) => value.as_str(),
    }
}

pub(crate) enum Replacement {
    Text(String),
    Group(usize),
}

/// Parse RDFLib's dollar-to-backslash conversion and Python replacement grammar.
pub(crate) fn replacement(source: &str, regex: &Regex) -> Result<Vec<Replacement>> {
    // re.sub(r"\$([0-9]*)", r"\\\1", source) is exactly a dollar-to-backslash
    // substitution: the digit suffix is copied unchanged, even when empty.
    let converted = source.replace('$', "\\");
    let mut chars = converted.chars().peekable();
    let mut pieces = Vec::new();
    let mut text = String::new();
    let invalid =
        |message: &str| Error::Invalid(format!("Invalid SPARQL REPLACE replacement: {message}"));
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            text.push(ch);
            continue;
        }
        let ch = chars.next().ok_or_else(|| invalid("trailing backslash"))?;
        let index = match ch {
            'g' => {
                if chars.next() != Some('<') {
                    return Err(invalid("missing < in group reference"));
                }
                let mut name = String::new();
                loop {
                    match chars.next() {
                        Some('>') => break,
                        Some(ch) => name.push(ch),
                        None => return Err(invalid("unterminated group reference")),
                    }
                }
                if name.is_empty() {
                    return Err(invalid("empty group reference"));
                }
                if name.chars().all(|ch| ch.is_ascii_digit()) {
                    name.parse()
                        .map_err(|_| invalid("group number is too large"))?
                } else {
                    regex
                        .capture_names()
                        .position(|candidate| candidate == Some(name.as_str()))
                        .ok_or_else(|| invalid("unknown group name"))?
                }
            }
            '0'..='9' => {
                let mut digits = String::from(ch);
                if ch == '0' {
                    for _ in 0..2 {
                        if chars.peek().is_some_and(|ch| ('0'..='7').contains(ch)) {
                            digits.push(chars.next().unwrap());
                        } else {
                            break;
                        }
                    }
                    text.push(char::from(
                        u8::from_str_radix(&digits, 8)
                            .expect("three octal digits starting with zero"),
                    ));
                    continue;
                }
                if chars.peek().is_some_and(char::is_ascii_digit) {
                    digits.push(chars.next().unwrap());
                }
                if digits.len() == 2
                    && digits.chars().all(|ch| ('0'..='7').contains(&ch))
                    && chars.peek().is_some_and(|ch| ('0'..='7').contains(ch))
                {
                    digits.push(chars.next().unwrap());
                    text.push(char::from(
                        u8::from_str_radix(&digits, 8)
                            .map_err(|_| invalid("octal escape exceeds 0o377"))?,
                    ));
                    continue;
                }
                digits.parse().expect("one or two ASCII digits")
            }
            _ => {
                match ch {
                    '\\' => text.push('\\'),
                    'a' => text.push('\x07'),
                    'b' => text.push('\x08'),
                    'f' => text.push('\x0c'),
                    'n' => text.push('\n'),
                    'r' => text.push('\r'),
                    't' => text.push('\t'),
                    'v' => text.push('\x0b'),
                    ch if ch.is_ascii_alphabetic() => return Err(invalid("unknown escape")),
                    ch => {
                        text.push('\\');
                        text.push(ch);
                    }
                }
                continue;
            }
        };
        if index >= regex.captures_len() {
            return Err(invalid("invalid group reference"));
        }
        pieces.push(Replacement::Text(std::mem::take(&mut text)));
        pieces.push(Replacement::Group(index));
    }
    pieces.push(Replacement::Text(text));
    Ok(pieces)
}
