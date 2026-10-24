//! Explicit host tables are independent of Rust's Unicode version.

use pyshacl_turbo_engine::{RegexClass, RegexClasses, RegexPolicy};

#[test]
fn host_tables_are_owned_compressed_and_do_not_reenter_scalar_callbacks() {
    let policy = RegexPolicy::from_scalars(
        |ch| {
            RegexClasses::new(
                ch.is_ascii_digit(),
                ch.is_ascii_alphanumeric() || ch == '_',
                ch == ' ',
            )
        },
        |ch| ch.to_ascii_lowercase(),
        vec![('i', vec!['ı'])],
        false,
    );
    assert!(policy.contains(RegexClass::Digit, '5'));
    assert!(!policy.contains(RegexClass::Digit, '\u{1e5f1}'));
    assert!(policy.contains(RegexClass::Word, '_'));
    assert!(!policy.contains(RegexClass::Word, '\u{1c89}'));
    assert_eq!(policy.simple_lower('I'), 'i');
    assert_eq!(policy.simple_lower('\u{1c89}'), '\u{1c89}');
    assert_eq!(policy.extra_cases('i'), &['ı']);
    assert!(policy.extra_cases('x').is_empty());
    assert!(!policy.empty_nonboundary());
    assert!(policy.allocated_bytes() < 1024);
    std::thread::spawn(move || {
        assert!(policy.contains(RegexClass::Space, ' '));
        assert_eq!(policy.simple_lower('Z'), 'z');
    })
    .join()
    .unwrap();
}

#[test]
fn host_classes_and_lower_mapping_preserve_scalar_domain_boundaries() {
    let policy = RegexPolicy::from_scalars(
        |ch| {
            RegexClasses::new(
                matches!(ch, '\0' | '\u{d7ff}' | '\u{e000}' | '\u{10ffff}'),
                false,
                false,
            )
        },
        |ch| if ch == '\u{10ffff}' { '\0' } else { ch },
        Vec::new(),
        true,
    );
    for ch in ['\0', '\u{d7ff}', '\u{e000}', '\u{10ffff}'] {
        assert!(policy.contains(RegexClass::Digit, ch));
    }
    assert!(!policy.contains(RegexClass::Digit, '\u{10fffe}'));
    assert_eq!(policy.simple_lower('\u{10ffff}'), '\0');
    assert!(policy.empty_nonboundary());
}

#[test]
fn each_host_callback_runs_once_per_scalar_and_is_not_retained() {
    let mut classified = 0usize;
    let mut lowercased = 0usize;
    let policy = RegexPolicy::from_scalars(
        |_| {
            classified += 1;
            RegexClasses::new(false, false, false)
        },
        |ch| {
            lowercased += 1;
            ch
        },
        vec![('z', vec!['b', 'a', 'b']), ('a', vec!['z'])],
        false,
    );
    assert_eq!(classified, 0x110000 - 0x800);
    assert_eq!(lowercased, classified);
    for _ in 0..20 {
        assert!(!policy.contains(RegexClass::Word, 'a'));
        assert_eq!(policy.simple_lower('A'), 'A');
    }
    assert_eq!(policy.extra_cases('z'), &['a', 'b']);
    assert_eq!(policy.extra_cases('a'), &['z']);
    assert_eq!(classified, lowercased);
}

#[test]
#[should_panic(expected = "Duplicate regex case exception key")]
fn duplicate_exception_authority_is_rejected() {
    RegexPolicy::from_scalars(
        |_| RegexClasses::new(false, false, false),
        |ch| ch,
        vec![('a', vec!['b']), ('a', vec!['c'])],
        false,
    );
}

#[derive(Debug)]
struct BackreferencePolicy {
    new_case_pair: bool,
}

impl fancy_regex::PythonCaseMapping for BackreferencePolicy {
    fn simple_lower(&self, ch: char) -> char {
        match ch {
            'É' => 'é',
            '\u{130}' => 'i',
            '\u{212a}' => 'k',
            '\u{1c89}' if self.new_case_pair => '\u{1c8a}',
            _ => ch.to_ascii_lowercase(),
        }
    }
}

#[test]
fn backreference_policy_is_owned_and_distinct_from_literal_case_closure() {
    use std::sync::Arc;
    for new_case_pair in [false, true] {
        let policy = Arc::new(BackreferencePolicy { new_case_pair });
        let weak = Arc::downgrade(&policy);
        let regex = fancy_regex::RegexBuilder::new(r"(?i)^(.)\1$")
            .python_case_mapping(policy.clone())
            .build()
            .unwrap();
        for pattern in [r"(?ai)^(.)\1$", r"(?i)^(?a:(?P<a>.)(?P=a))$"] {
            let ascii = fancy_regex::RegexBuilder::new(pattern)
                .python_case_mapping(policy.clone())
                .build()
                .unwrap();
            for (text, expected) in [
                ("aA", true),
                ("iİ", false),
                ("iı", false),
                ("sſ", false),
                ("kK", false),
                ("Éé", false),
                ("\u{1c89}\u{1c8a}", false),
            ] {
                assert_eq!(
                    ascii.is_match(text).unwrap(),
                    expected,
                    "{pattern:?}: {text:?}"
                );
            }
        }
        drop(policy);
        for (text, expected) in [
            ("iİ", true),
            ("iı", false),
            ("sſ", false),
            ("kK", true),
            ("Éé", true),
            ("\u{1c89}\u{1c8a}", new_case_pair),
        ] {
            assert_eq!(regex.is_match(text).unwrap(), expected, "{text:?}");
        }
        // A literal/set has a larger Python case closure than a backreference.
        assert!(
            fancy_regex::Regex::new("^[iı]$")
                .unwrap()
                .is_match("ı")
                .unwrap()
        );
        assert!(
            fancy_regex::Regex::new("^[sſ]$")
                .unwrap()
                .is_match("ſ")
                .unwrap()
        );
        assert!(weak.upgrade().is_some());
        let clone = regex.clone();
        drop(regex);
        assert!(clone.is_match("iİ").unwrap());
        drop(clone);
        assert!(weak.upgrade().is_none());
    }
}

#[test]
fn host_literal_case_closure_uses_compressed_mapping_and_finite_exceptions() {
    use regex_syntax::hir::{ClassUnicode, ClassUnicodeRange};
    let class = |start, end| ClassUnicode::new([ClassUnicodeRange::new(start, end)]);
    let contains = |set: &ClassUnicode, ch| {
        set.ranges()
            .iter()
            .any(|range| range.start() <= ch && ch <= range.end())
    };
    for new_pair in [false, true] {
        let policy = RegexPolicy::from_scalars(
            |ch| {
                RegexClasses::new(
                    ch.is_ascii_digit(),
                    ch.is_ascii_alphanumeric() || ch == '_',
                    ch == ' ',
                )
            },
            |ch| match ch {
                'İ' => 'i',
                'K' => 'k',
                '\u{1c89}' if new_pair => '\u{1c8a}',
                _ => ch.to_ascii_lowercase(),
            },
            vec![
                ('i', vec!['ı']),
                ('ı', vec!['i']),
                ('s', vec!['ſ']),
                ('ſ', vec!['s']),
            ],
            false,
        );
        let letters = policy.fold_class(&class('A', 'Z'));
        for ch in ['A', 'Z', 'a', 'z', 'İ', 'ı', 'ſ', 'K'] {
            assert!(contains(&letters, ch), "missing {ch:?}");
        }
        assert!(!contains(&letters, '\u{1c89}'));
        let new_letter = policy.fold_class(&class('\u{1c8a}', '\u{1c8a}'));
        assert_eq!(contains(&new_letter, '\u{1c89}'), new_pair);
        assert!(contains(&new_letter, '\u{1c8a}'));
        let digits = policy.character_class(RegexClass::Digit);
        assert!(contains(&digits, '5'));
        assert!(!contains(&digits, '\u{1e5f1}'));
        let all = policy.fold_class(&class('\0', '\u{10ffff}'));
        assert_eq!(all, class('\0', '\u{10ffff}'));
        assert!(
            policy
                .fold_class(&ClassUnicode::empty())
                .ranges()
                .is_empty()
        );
    }
}
