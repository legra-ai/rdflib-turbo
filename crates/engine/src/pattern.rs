//! pySHACL Core uses Python's `re`, including its non-SPARQL flag semantics.
//! Translate lexical differences once; matching and captures remain in Rust.

use fancy_regex::{Regex, RegexBuilder, RegexInput};
use regex_syntax::hir::{Class, ClassUnicode, ClassUnicodeRange, HirKind};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};

use crate::{Error, RegexClass, RegexLexicalResolver, RegexPolicy, Result};

#[cfg(test)]
#[path = "pattern_host_tests.rs"]
mod host_tests;

pub(crate) struct Pattern {
    regex: Regex,
    translated: String,
    nonempty: OnceLock<std::result::Result<Option<Regex>, String>>,
    policy: Option<Arc<RegexPolicy>>,
}

impl Pattern {
    pub(crate) fn compile(pattern: &str, flags: &str) -> Result<Self> {
        // pySHACL lowercases sh:flags and recognizes only i and m here. Its
        // SPARQL REGEX implementation has a different contract.
        let flags = flags.to_lowercase();
        Self::compile_flags(
            pattern,
            Flags {
                insensitive: flags.contains('i'),
                multiline: flags.contains('m'),
                ..Flags::default()
            },
            None,
        )
    }

    pub(crate) fn compile_host(
        pattern: &str,
        flags: &str,
        policy: Arc<RegexPolicy>,
        lexical: &dyn RegexLexicalResolver,
    ) -> Result<Self> {
        let flags = flags.to_lowercase();
        Self::compile_flags(
            pattern,
            Flags {
                insensitive: flags.contains('i'),
                multiline: flags.contains('m'),
                ..Flags::default()
            },
            Some((policy, lexical)),
        )
    }

    pub(crate) fn compile_sparql(pattern: &str, flags: &str) -> Result<Self> {
        Self::compile_flags(
            pattern,
            Flags {
                insensitive: flags.contains('i'),
                multiline: flags.contains('m'),
                dotall: flags.contains('s'),
                ..Flags::default()
            },
            None,
        )
        .map_err(|e| Error::Invalid(e.to_string().replace("sh:pattern", "SPARQL pattern")))
    }

    fn compile_flags(
        pattern: &str,
        flags: Flags,
        host: Option<(Arc<RegexPolicy>, &dyn RegexLexicalResolver)>,
    ) -> Result<Self> {
        let mut parser = Translator {
            input: pattern.chars().collect(),
            offset: 0,
            flags,
            stack: Vec::new(),
            names: HashSet::new(),
            at_start: true,
            global_charset: None,
            last_assertion: false,
            host: host
                .as_ref()
                .map(|(policy, lexical)| (policy.as_ref(), *lexical)),
            aliases: HashMap::new(),
        };
        let translated = parser.translate()?;
        let mut builder = RegexBuilder::new(&translated);
        if let Some((policy, _)) = &host {
            builder.python_case_mapping(policy.clone());
        }
        let regex = builder
            .build()
            .map_err(|e| Error::Invalid(format!("Invalid sh:pattern: {e}")))?;
        Ok(Self {
            regex,
            translated,
            nonempty: OnceLock::new(),
            policy: host.map(|(policy, _)| policy),
        })
    }

    pub(crate) fn is_match(&self, text: &str) -> Result<bool> {
        // Backtracking exhaustion is an execution error, never a non-match.
        self.regex
            .is_match(text)
            .map_err(|e| Error::Runtime(format!("sh:pattern matching failed: {e}")))
    }

    pub(crate) fn replace(&self, text: &str, replacement: &str, count: usize) -> Result<String> {
        let template = crate::sparql_regex::replacement(replacement, &self.regex)?;
        let mut output = String::new();
        let (mut search, mut copied, mut replaced, mut after_empty) = (0, 0, 0, false);
        while search <= text.len() && (count == 0 || replaced < count) {
            // Since Python 3.7, a nonempty match may follow an empty match at
            // the same position. Retry that position with empty matches barred
            // before advancing by one Unicode scalar. Do not slice the input:
            // anchors and lookbehind must still see the original string.
            let captures = if after_empty {
                let nonempty = self.nonempty.get_or_init(|| {
                    let mut builder = RegexBuilder::new(&self.translated);
                    builder.find_not_empty(true);
                    if let Some(policy) = &self.policy {
                        builder.python_case_mapping(policy.clone());
                    }
                    match builder.build() {
                        Ok(regex) => Ok(Some(regex)),
                        Err(fancy_regex::Error::CompileError(error))
                            if matches!(
                                *error,
                                fancy_regex::CompileError::PatternCanNeverMatch
                            ) =>
                        {
                            Ok(None)
                        }
                        Err(error) => Err(error.to_string()),
                    }
                });
                match nonempty {
                    Ok(Some(regex)) => {
                        regex.captures_input(RegexInput::new(text).from_pos(search).anchored(true))
                    }
                    Ok(None) => Ok(None),
                    Err(error) => {
                        return Err(Error::Runtime(format!(
                            "SPARQL REPLACE matching failed: {error}"
                        )));
                    }
                }
            } else {
                self.regex.captures_from_pos(text, search)
            }
            .map_err(|e| Error::Runtime(format!("SPARQL REPLACE matching failed: {e}")))?;
            let Some(captures) = captures else {
                if !after_empty || search == text.len() {
                    break;
                }
                search += text[search..]
                    .chars()
                    .next()
                    .expect("position before end")
                    .len_utf8();
                after_empty = false;
                continue;
            };
            let matched = captures.get(0).expect("whole match exists");
            output.push_str(&text[copied..matched.start()]);
            for piece in &template {
                match piece {
                    crate::sparql_regex::Replacement::Text(value) => output.push_str(value),
                    crate::sparql_regex::Replacement::Group(index) => {
                        if let Some(value) = captures.get(*index) {
                            output.push_str(value.as_str());
                        }
                    }
                }
            }
            search = matched.end();
            copied = search;
            after_empty = matched.start() == matched.end();
            replaced += 1;
        }
        output.push_str(&text[copied..]);
        Ok(output)
    }
}

#[derive(Clone, Copy, Default)]
struct Flags {
    insensitive: bool,
    multiline: bool,
    dotall: bool,
    verbose: bool,
    ascii: bool,
}

impl Flags {
    fn backend(self) -> String {
        let mut enabled = String::new();
        let mut disabled = String::new();
        for (flag, active) in [
            ('i', self.insensitive),
            ('m', self.multiline),
            ('s', self.dotall),
            ('a', self.ascii),
        ] {
            if active {
                enabled.push(flag);
            } else {
                disabled.push(flag);
            }
        }
        format!("{enabled}-{disabled}")
    }

    fn class(self, kind: char) -> String {
        let body = match (kind.to_ascii_lowercase(), self.ascii) {
            ('w', false) => r"\p{L}\p{N}_",
            ('w', true) => "a-zA-Z0-9_",
            ('d', false) => r"\p{Nd}",
            ('d', true) => "0-9",
            ('s', false) => r"\s\x1c-\x1f",
            ('s', true) => r" \t\n\r\f\v",
            _ => unreachable!("only character class escapes call Flags::class"),
        };
        let negation = if kind.is_ascii_uppercase() { "^" } else { "" };
        format!("[{negation}{body}]")
    }
}

struct Translator<'a> {
    input: Vec<char>,
    offset: usize,
    flags: Flags,
    stack: Vec<Flags>,
    names: HashSet<String>,
    at_start: bool,
    global_charset: Option<char>,
    last_assertion: bool,
    host: Option<(&'a RegexPolicy, &'a dyn RegexLexicalResolver)>,
    aliases: HashMap<String, String>,
}

impl Translator<'_> {
    fn invalid(&self, message: &str) -> Error {
        Error::Invalid(format!(
            "Invalid sh:pattern at character {}: {message}",
            self.offset
        ))
    }

    fn peek(&self) -> Option<char> {
        self.input.get(self.offset).copied()
    }

    fn next(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.offset += 1;
        Some(ch)
    }

    fn starts_with(&self, text: &str) -> bool {
        self.input[self.offset..]
            .iter()
            .copied()
            .zip(text.chars())
            .all(|(a, b)| a == b)
            && self.input.len() - self.offset >= text.chars().count()
    }

    fn translate(&mut self) -> Result<String> {
        let mut output = format!("(?{})", self.flags.backend());
        while let Some(ch) = self.next() {
            match ch {
                ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c' if self.flags.verbose => {}
                '#' if self.flags.verbose => while self.next().is_some_and(|ch| ch != '\n') {},
                '\\' => {
                    self.at_start = false;
                    self.last_assertion = self.peek().is_some_and(|ch| "AbBZ".contains(ch));
                    output.push_str(&self.escape(false)?);
                }
                '[' => {
                    self.at_start = false;
                    self.last_assertion = false;
                    output.push_str(&self.character_class()?);
                }
                '$' if !self.flags.multiline => {
                    self.at_start = false;
                    self.last_assertion = true;
                    output.push_str(r"(?=\n?\z)");
                }
                '(' => output.push_str(&self.group()?),
                ')' => {
                    self.last_assertion = false;
                    self.flags = self
                        .stack
                        .pop()
                        .ok_or_else(|| self.invalid("unmatched ')'"))?;
                    output.push(')');
                }
                '{' => {
                    self.at_start = false;
                    let rest = &self.input[self.offset..];
                    let mut end = 0;
                    while rest.get(end).is_some_and(char::is_ascii_digit) {
                        end += 1;
                    }
                    if rest.get(end) == Some(&',') {
                        end += 1;
                        while rest.get(end).is_some_and(char::is_ascii_digit) {
                            end += 1;
                        }
                    }
                    if rest.get(end) != Some(&'}') || end == 0 {
                        self.last_assertion = false;
                        output.push_str(r"\{");
                    } else if self.last_assertion {
                        return Err(self.invalid("nothing to repeat"));
                    } else if self.peek() == Some(',') {
                        output.push_str("{0");
                    } else {
                        output.push('{');
                    }
                }
                _ => {
                    self.at_start = false;
                    if self.last_assertion && "*+?".contains(ch) {
                        return Err(self.invalid("nothing to repeat"));
                    }
                    self.last_assertion = ch == '^' || ch == '$';
                    if self.flags.insensitive && ch.is_alphabetic() {
                        output.push_str(&self.literal(ch)?);
                    } else {
                        output.push(ch);
                    }
                }
            }
        }
        if !self.stack.is_empty() {
            return Err(self.invalid("unterminated group"));
        }
        Ok(output)
    }

    fn group(&mut self) -> Result<String> {
        if self.starts_with("?#") {
            self.offset += 2;
            loop {
                match self.next() {
                    Some(')') => return Ok(String::new()),
                    Some('\\') => {
                        self.next();
                    }
                    Some(_) => {}
                    None => return Err(self.invalid("unterminated comment")),
                }
            }
        }
        if self.peek() == Some('?')
            && self
                .input
                .get(self.offset + 1)
                .is_some_and(|ch| "aiLmsux-".contains(*ch))
        {
            self.offset += 1;
            let old = self.flags;
            let mut disabling = false;
            let mut ascii_or_unicode = None;
            let mut enabled = HashSet::new();
            let mut disabled = HashSet::new();
            loop {
                match self.next() {
                    Some(':') => {
                        if disabling && disabled.is_empty() {
                            return Err(self.invalid("missing disabled flags"));
                        }
                        self.at_start = false;
                        self.last_assertion = false;
                        self.stack.push(old);
                        return Ok(format!("(?{}:", self.flags.backend()));
                    }
                    Some(')') if !disabling && self.at_start => {
                        if let Some(charset) = ascii_or_unicode {
                            if self.global_charset.is_some_and(|old| old != charset) {
                                return Err(
                                    self.invalid("ASCII and Unicode flags are incompatible")
                                );
                            }
                            self.global_charset = Some(charset);
                        }
                        return Ok(format!("(?{})", self.flags.backend()));
                    }
                    Some('-') if !disabling => disabling = true,
                    Some(ch @ ('a' | 'u')) if !disabling => {
                        if ascii_or_unicode.is_some_and(|previous| previous != ch) {
                            return Err(self.invalid("ASCII and Unicode flags are incompatible"));
                        }
                        ascii_or_unicode = Some(ch);
                        self.flags.ascii = ch == 'a';
                    }
                    Some(ch @ ('i' | 'm' | 's' | 'x')) => {
                        if disabling {
                            if enabled.contains(&ch) {
                                return Err(self.invalid("flag is both enabled and disabled"));
                            }
                            disabled.insert(ch);
                        } else {
                            enabled.insert(ch);
                        }
                        match ch {
                            'i' => self.flags.insensitive = !disabling,
                            'm' => self.flags.multiline = !disabling,
                            's' => self.flags.dotall = !disabling,
                            'x' => self.flags.verbose = !disabling,
                            _ => unreachable!(),
                        }
                    }
                    _ => return Err(self.invalid("invalid inline flags")),
                }
            }
        }
        self.at_start = false;
        self.last_assertion = false;
        self.stack.push(self.flags);
        // Consume headers so group names and condition numbers cannot be
        // interpreted as pattern text or nested groups by this translator.
        for prefix in ["?P<", "?P=", "?("] {
            if self.starts_with(prefix) {
                self.offset += prefix.len();
                let mut header = format!("({prefix}");
                let end = if prefix == "?P<" { '>' } else { ')' };
                loop {
                    let ch = self
                        .next()
                        .ok_or_else(|| self.invalid("unterminated group header"))?;
                    header.push(ch);
                    if ch == end {
                        break;
                    }
                }
                let name = &header[1 + prefix.len()..header.len() - 1];
                if prefix == "?P=" {
                    self.stack.pop();
                }
                if let Some((_, lexical)) = self.host {
                    let numeric = !name.is_empty() && name.chars().all(|ch| ch.is_ascii_digit());
                    if prefix == "?P<" {
                        if !lexical.is_identifier(name)? || !self.names.insert(name.to_owned()) {
                            return Err(self.invalid("invalid or duplicate group name"));
                        }
                        self.aliases
                            .insert(name.to_owned(), format!("g{}", self.names.len()));
                    } else if (prefix != "?(" || !numeric) && !lexical.is_identifier(name)? {
                        return Err(self.invalid("invalid group name"));
                    }
                    if prefix == "?(" && numeric {
                        return Ok(header);
                    }
                    // Internal ASCII names prevent the delegate parser from
                    // imposing another Unicode version after host validation.
                    let alias = self
                        .aliases
                        .get(name)
                        .ok_or_else(|| self.invalid("unknown group name"))?;
                    return Ok(format!("({prefix}{alias}{end}"));
                }
                if prefix == "?P<" {
                    let mut chars = name.chars();
                    if !chars
                        .next()
                        .is_some_and(|ch| ch == '_' || unicode_ident::is_xid_start(ch))
                        || !chars.all(unicode_ident::is_xid_continue)
                        || !self.names.insert(name.to_owned())
                    {
                        return Err(self.invalid("invalid or duplicate group name"));
                    }
                }
                if prefix == "?("
                    && !self.names.contains(name)
                    && (name.is_empty() || !name.chars().all(|ch| ch.is_ascii_digit()))
                {
                    return Err(self.invalid("unknown conditional group name"));
                }
                return Ok(header);
            }
        }
        if self.peek() == Some('?') {
            let valid = ["?:", "?=", "?!", "?<=", "?<!", "?>"]
                .iter()
                .any(|p| self.starts_with(p));
            if !valid {
                return Err(self.invalid("unknown group extension"));
            }
        }
        Ok("(".into())
    }

    fn literal(&self, ch: char) -> Result<String> {
        self.fold_class(&format!(r"[\x{{{:x}}}]", u32::from(ch)), false)
    }

    fn class(&self, kind: char) -> String {
        let Some((policy, _)) = self.host.filter(|_| !self.flags.ascii) else {
            return self.flags.class(kind);
        };
        let mut class = policy.character_class(match kind.to_ascii_lowercase() {
            'w' => RegexClass::Word,
            'd' => RegexClass::Digit,
            's' => RegexClass::Space,
            _ => unreachable!("only category escapes"),
        });
        if kind.is_ascii_uppercase() {
            class.negate();
        }
        Self::class_text(&class)
    }

    fn class_text(class: &ClassUnicode) -> String {
        if class.ranges().is_empty() {
            return r"[^\x{0}-\x{10ffff}]".into();
        }
        let mut output = String::from("[");
        for range in class.ranges() {
            output.push_str(&format!(r"\x{{{:x}}}", u32::from(range.start())));
            if range.start() != range.end() {
                output.push_str(&format!(r"-\x{{{:x}}}", u32::from(range.end())));
            }
        }
        output.push(']');
        output
    }

    fn fold_class(&self, text: &str, negate: bool) -> Result<String> {
        self.fold_class_with_categories(text, negate, &ClassUnicode::empty())
    }

    fn parse_class(&self, text: &str) -> Result<ClassUnicode> {
        if text == "[]" {
            return Ok(ClassUnicode::empty());
        }
        let hir = regex_syntax::Parser::new()
            .parse(text)
            .map_err(|e| self.invalid(&e.to_string()))?;
        Ok(match hir.kind() {
            HirKind::Class(Class::Unicode(class)) => class.clone(),
            HirKind::Literal(value) => {
                let value = std::str::from_utf8(&value.0)
                    .map_err(|_| self.invalid("invalid class character"))?;
                let ch = value
                    .chars()
                    .next()
                    .ok_or_else(|| self.invalid("empty class"))?;
                ClassUnicode::new([ClassUnicodeRange::new(ch, ch)])
            }
            _ => return Err(self.invalid("invalid character class")),
        })
    }

    fn fold_class_with_categories(
        &self,
        text: &str,
        negate: bool,
        categories: &ClassUnicode,
    ) -> Result<String> {
        let mut class = self.parse_class(text)?;
        if self.flags.insensitive {
            if self.flags.ascii {
                for ch in 'a'..='z' {
                    let upper = ch.to_ascii_uppercase();
                    if class.ranges().iter().any(|r| {
                        (r.start()..=r.end()).contains(&ch)
                            || (r.start()..=r.end()).contains(&upper)
                    }) {
                        class.push(ClassUnicodeRange::new(ch, ch));
                        class.push(ClassUnicodeRange::new(upper, upper));
                    }
                }
            } else if let Some((policy, _)) = self.host {
                class = policy.fold_class(&class);
            } else {
                class.case_fold_simple();
                // Python's case-insensitive literal/set matching includes the
                // dotted and dotless I in addition to Unicode simple folding.
                if ['i', 'I', 'İ', 'ı'].iter().any(|ch| {
                    class
                        .ranges()
                        .iter()
                        .any(|r| (r.start()..=r.end()).contains(ch))
                }) {
                    for ch in ['i', 'I', 'İ', 'ı'] {
                        class.push(ClassUnicodeRange::new(ch, ch));
                    }
                }
            }
        }
        // Category predicates are not literal case closures. In particular
        // IGNORECASE [\w] must not include the combining Greek U+0345.
        class.union(categories);
        if negate {
            class.negate();
        }
        Ok(format!("(?-i:{})", Self::class_text(&class)))
    }

    fn character_class(&mut self) -> Result<String> {
        let mut output = String::from("[");
        let negate = self.peek() == Some('^');
        if negate {
            self.offset += 1;
        }
        if self.peek() == Some(']') {
            self.offset += 1;
            output.push_str(r"\]");
        }
        let mut previous_class = false;
        let mut categories = ClassUnicode::empty();
        loop {
            match self.next() {
                Some(']') => {
                    output.push(']');
                    return self.fold_class_with_categories(&output, negate, &categories);
                }
                Some('\\') => {
                    let is_class = self.peek().is_some_and(|ch| "wWdDsS".contains(ch));
                    if is_class && output.ends_with('-') && output.len() > 2 {
                        return Err(self.invalid("character class cannot end a range"));
                    }
                    if is_class && self.host.is_some() {
                        let kind = self.next().expect("peeked category");
                        categories.union(&self.parse_class(&self.class(kind))?);
                    } else {
                        output.push_str(&self.escape(true)?);
                    }
                    previous_class = is_class;
                }
                Some('-') if previous_class && self.peek() != Some(']') => {
                    return Err(self.invalid("character class cannot start a range"));
                }
                // Python classes have no nested sets or set operations.
                Some(ch @ ('[' | '&' | '~' | '|')) => {
                    previous_class = false;
                    output.push('\\');
                    output.push(ch);
                }
                Some(ch) => {
                    previous_class = false;
                    output.push(ch);
                }
                None => return Err(self.invalid("unterminated character class")),
            }
        }
    }

    fn escape(&mut self, in_class: bool) -> Result<String> {
        let ch = self
            .next()
            .ok_or_else(|| self.invalid("trailing backslash"))?;
        match ch {
            'w' | 'W' | 'd' | 'D' | 's' | 'S' => {
                let class = self.class(ch);
                Ok(if in_class {
                    class
                } else {
                    format!("(?-i:{class})")
                })
            }
            'Z' if !in_class => Ok(r"\z".into()),
            'b' | 'B' if !in_class => {
                let word = format!("(?-i:{})", self.class('w'));
                if ch == 'b' {
                    Ok(format!("(?:(?<={word})(?!{word})|(?<!{word})(?={word}))"))
                } else {
                    let guard = if self
                        .host
                        .is_some_and(|(policy, _)| policy.empty_nonboundary())
                    {
                        ""
                    } else {
                        r"(?!\A\z)"
                    };
                    Ok(format!(
                        r"{guard}(?:(?<={word})(?={word})|(?<!{word})(?!{word}))"
                    ))
                }
            }
            '0'..='9' => {
                let mut digits = String::from(ch);
                let octal = in_class
                    || ch == '0'
                    || (ch <= '7'
                        && self
                            .input
                            .get(self.offset..self.offset + 2)
                            .is_some_and(|chars| chars.iter().all(|ch| ('0'..='7').contains(ch))));
                if octal {
                    while digits.len() < 3
                        && self.peek().is_some_and(|ch| ('0'..='7').contains(&ch))
                    {
                        digits.push(self.next().expect("peeked digit"));
                    }
                    let code = u32::from_str_radix(&digits, 8)
                        .map_err(|_| self.invalid("invalid octal escape"))?;
                    if code > 0xff {
                        return Err(self.invalid("octal escape is outside 0o377"));
                    }
                    if in_class {
                        Ok(format!(r"\x{{{code:x}}}"))
                    } else {
                        self.literal(char::from_u32(code).expect("octal <= 255"))
                    }
                } else {
                    if self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
                        digits.push(self.next().expect("peeked digit"));
                    }
                    Ok(format!(r"\k<{digits}>"))
                }
            }
            'x' | 'u' | 'U' => {
                let count = match ch {
                    'x' => 2,
                    'u' => 4,
                    _ => 8,
                };
                let mut digits = String::new();
                for _ in 0..count {
                    let ch = self
                        .next()
                        .filter(char::is_ascii_hexdigit)
                        .ok_or_else(|| self.invalid("incomplete hexadecimal escape"))?;
                    digits.push(ch);
                }
                let code = u32::from_str_radix(&digits, 16)
                    .map_err(|_| self.invalid("invalid hexadecimal escape"))?;
                let ch = char::from_u32(code)
                    .ok_or_else(|| self.invalid("invalid Unicode scalar value"))?;
                if in_class {
                    Ok(format!(r"\x{{{digits}}}"))
                } else {
                    self.literal(ch)
                }
            }
            'N' => {
                if self.next() != Some('{') {
                    return Err(self.invalid("missing Unicode character name"));
                }
                let mut name = String::new();
                loop {
                    match self.next() {
                        Some('}') => break,
                        Some(ch) => name.push(ch),
                        None => return Err(self.invalid("unterminated Unicode character name")),
                    }
                }
                let ch = match self.host {
                    Some((_, lexical)) => lexical.named_character(&name)?,
                    None => unicode_names2::character(&name),
                }
                .ok_or_else(|| self.invalid("unknown Unicode character name"))?;
                if in_class {
                    Ok(format!(r"\x{{{:x}}}", u32::from(ch)))
                } else {
                    self.literal(ch)
                }
            }
            'A' if !in_class => Ok(r"\A".into()),
            'b' if in_class => Ok(r"\x08".into()),
            'a' | 'b' | 'f' | 'n' | 'r' | 't' | 'v' => Ok(format!(r"\{ch}")),
            ch if ch.is_ascii_alphabetic() => Err(self.invalid("unknown escape")),
            // Escape as a codepoint: fancy-regex assigns special meanings to
            // some punctuation escapes (for example \< and \>).
            ch => {
                if in_class {
                    Ok(format!(r"\x{{{:x}}}", u32::from(ch)))
                } else {
                    self.literal(ch)
                }
            }
        }
    }
}
