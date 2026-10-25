use super::*;
use crate::{RegexClasses, RegexLexicalResolver, RegexPolicy};
use std::sync::Arc;

struct Lexical {
    new_unicode: bool,
}
impl RegexLexicalResolver for Lexical {
    fn is_identifier(&self, name: &str) -> Result<bool> {
        Ok(match name {
            "a\u{301}" => true,
            "\u{1c89}" => self.new_unicode,
            _ => !name.is_empty() && name.chars().all(|ch| ch.is_ascii_alphabetic() || ch == '_'),
        })
    }
    fn named_character(&self, name: &str) -> Result<Option<char>> {
        Ok(match name {
            "LATIN SMALL LETTER A" => Some('a'),
            _ => None,
        })
    }
}

fn policy(new_unicode: bool) -> Arc<RegexPolicy> {
    Arc::new(RegexPolicy::from_scalars(
        |ch| {
            RegexClasses::new(
                ch.is_ascii_digit() || (new_unicode && ch == '\u{1e5f1}'),
                ch.is_ascii_alphanumeric()
                    || matches!(ch, '_' | 'ι')
                    || (new_unicode && ch == '\u{1e5f1}'),
                ch.is_ascii_whitespace() || ch == '\u{1c}',
            )
        },
        |ch| match ch {
            'İ' => 'i',
            'K' => 'k',
            'Ι' => 'ι',
            '\u{1c89}' if new_unicode => '\u{1c8a}',
            _ => ch.to_ascii_lowercase(),
        },
        vec![
            ('i', vec!['ı']),
            ('ı', vec!['i']),
            ('s', vec!['ſ']),
            ('ſ', vec!['s']),
            ('ι', vec!['\u{345}']),
            ('\u{345}', vec!['ι']),
        ],
        new_unicode,
    ))
}

#[test]
fn host_categories_boundaries_flags_and_case_closure_have_no_rust_unicode_fallback() {
    for newer in [false, true] {
        let policy = policy(newer);
        let lexical = Lexical { new_unicode: newer };
        for (pattern, text, flags, expected) in [
            (r"^\d$", "\u{1e5f1}", "", newer),
            (r"^\w$", "\u{1e5f1}", "", newer),
            (r"^[^\w]$", "\u{1e5f1}", "", !newer),
            (r"^\B.\B$", "\u{1e5f1}", "", !newer),
            (r"(?a:^\d$)", "\u{1e5f1}", "", false),
            (r"\B", "", "", newer),
            ("^\u{1c89}$", "\u{1c8a}", "i", newer),
            ("^[\u{1c89}]$", "\u{1c8a}", "i", newer),
            (r"^(.)\1$", "\u{1c89}\u{1c8a}", "i", newer),
            (r"^\w$", "\u{345}", "i", false),
            (r"^[\w]$", "\u{345}", "i", false),
            (r"^[^\w]$", "\u{345}", "i", true),
            (r"^[\wA]$", "\u{345}", "i", false),
            (r"^[\wι]$", "\u{345}", "i", true),
            (r"^[\W]$", "\u{345}", "i", true),
        ] {
            let compiled = Pattern::compile_host(pattern, flags, policy.clone(), &lexical).unwrap();
            assert_eq!(
                compiled.is_match(text).unwrap(),
                expected,
                "{pattern:?} on{text:?}; policy{newer}"
            );
        }
    }
}

#[test]
fn identifiers_are_checked_at_their_token_before_later_syntax_errors() {
    let policy = policy(false);
    let lexical = Lexical { new_unicode: false };
    for (pattern, message) in [
        ("(?P<bad-name>x)[", "invalid or duplicate group name"),
        (r"\N{UNKNOWN}[", "unknown Unicode character name"),
    ] {
        let error = Pattern::compile_host(pattern, "", policy.clone(), &lexical)
            .err()
            .unwrap();
        assert!(error.to_string().contains(message), "{error}");
    }
    assert!(Pattern::compile_host("(?P<\u{1c89}>x)", "", policy.clone(), &lexical).is_err());
    let newer = Lexical { new_unicode: true };
    assert!(
        Pattern::compile_host("(?P<\u{1c89}>x)", "", policy, &newer)
            .unwrap()
            .is_match("x")
            .unwrap()
    );
}

#[test]
fn valid_combining_group_names_and_borrowed_resolvers_do_not_enter_compiled_ir() {
    let pattern = {
        let lexical = Lexical { new_unicode: false };
        Pattern::compile_host(
            "^(?P<a\u{301}>x)(?P=a\u{301})$",
            "",
            policy(false),
            &lexical,
        )
        .unwrap()
    };
    assert!(pattern.is_match("xx").unwrap());
    assert!(!pattern.is_match("xy").unwrap());
}

#[test]
fn compiled_shape_constraints_receive_the_required_host_policy() {
    use crate::oxigraph::io::RdfFormat;
    use crate::{CompiledShapes, IndexedShapesSource, Options, parse_graph};
    let shapes = "@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:S> sh:targetNode \"\u{1e5f1}\"; sh:pattern \"^\\\\d$\".";
    for newer in [false, true] {
        let compiled = {
            let lexical = Lexical { new_unicode: newer };
            CompiledShapes::compile_with_host_policy(
                IndexedShapesSource::parse_graph(shapes, RdfFormat::Turtle, None).unwrap(),
                false,
                true,
                &[],
                |_| None,
                policy(newer),
                &lexical,
            )
            .unwrap()
        };
        let mut data = parse_graph("", RdfFormat::Turtle, None).unwrap();
        assert_eq!(
            compiled
                .validate(&mut data, Options::default())
                .unwrap()
                .conforms,
            newer
        );
    }
}

#[test]
fn dependency_upgrade_preserves_python_capture_and_lookbehind_rejections() {
    let lexical = Lexical { new_unicode: false };
    for pattern in [
        r"(?P<a>x)(?P<a>y)",
        r"(?<=a|bc)x",
        r"(?<!a|bc)x",
        r"(?<=(a+)\1)x",
        r"(?P<a>x)(?P=unknown)",
    ] {
        assert!(
            Pattern::compile_host(pattern, "", policy(false), &lexical).is_err(),
            "Python-incompatible syntax accepted: {pattern}"
        );
    }
    for (pattern, text, expected) in [
        (r"(?P<a>x){0}", "", true),
        (r"(?=x)+x", "x", true),
        (r"(?<=a)x", "ax", true),
        (r"(?<=a|b)x", "bx", true),
        (r"(?<=^a)x", "ax", true),
        (r"(a)(?<=\1)x", "ax", true),
        (r"(?i)^(.)\1$", "iİ", true),
        (r"(?i)^(.)\1$", "iı", false),
        (r"(?i)^(.)\1$", "sſ", false),
        (r"(?ai)^(.)\1$", "iİ", false),
        (r"(?ai)^(?P<a>.)(?P=a)$", "Ιι", false),
    ] {
        let compiled = Pattern::compile_host(pattern, "", policy(false), &lexical).unwrap();
        assert_eq!(
            compiled.is_match(text).unwrap(),
            expected,
            "{pattern} {text}"
        );
    }
}
