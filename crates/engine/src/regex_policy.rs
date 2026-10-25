//! Immutable host scalar tables. Construction calls the host once per scalar;
//! native compilation and matching retain no host callback or Python object.

use regex_syntax::hir::{ClassUnicode, ClassUnicodeRange};

/// Temporary host lexical authority, borrowed only during pattern compilation.
/// Compiled programs must retain resolved scalars, never this resolver.
pub trait RegexLexicalResolver: Send + Sync {
    fn is_identifier(&self, name: &str) -> crate::Result<bool>;
    fn named_character(&self, name: &str) -> crate::Result<Option<char>>;
}

#[derive(Clone, Copy, Debug)]
pub enum RegexClass {
    Digit,
    Word,
    Space,
}

#[derive(Clone, Copy)]
pub struct RegexClasses([bool; 3]);

impl RegexClasses {
    pub fn new(digit: bool, word: bool, space: bool) -> Self {
        Self([digit, word, space])
    }
}

#[derive(Debug)]
struct Range {
    start: u32,
    end: u32,
}

#[derive(Debug)]
struct LowerRun {
    range: Range,
    delta: i32,
}

/// Owned, compressed data from one explicitly selected interpreter policy.
/// There is deliberately no default policy derived from Rust's Unicode tables.
#[derive(Debug)]
pub struct RegexPolicy {
    classes: [Box<[Range]>; 3],
    lower: Box<[LowerRun]>,
    extras: Box<[(char, Box<[char]>)]>,
    empty_nonboundary: bool,
}

impl fancy_regex::PythonCaseMapping for RegexPolicy {
    fn simple_lower(&self, ch: char) -> char {
        RegexPolicy::simple_lower(self, ch)
    }
}

impl RegexPolicy {
    fn class_ranges(&self, class: RegexClass) -> &[Range] {
        &self.classes[match class {
            RegexClass::Digit => 0,
            RegexClass::Word => 1,
            RegexClass::Space => 2,
        }]
    }

    /// Project the authoritative compressed category, without case folding it.
    pub fn character_class(&self, class: RegexClass) -> ClassUnicode {
        ClassUnicode::new(self.class_ranges(class).iter().map(|range| {
            ClassUnicodeRange::new(
                char::from_u32(range.start).expect("stored scalar start"),
                char::from_u32(range.end).expect("stored scalar end"),
            )
        }))
    }

    /// Python literal/range case closure, kept separate from category escapes.
    /// Work is proportional to compressed runs and intersections, never the
    /// full Unicode domain or the number of codepoints in a pattern's ranges.
    pub fn fold_class(&self, input: &ClassUnicode) -> ClassUnicode {
        let mut canonical = self.map_class(input, false);
        let mut exceptions = Vec::new();
        for (key, values) in &self.extras {
            let index = canonical
                .ranges()
                .partition_point(|range| range.end() < *key);
            if canonical
                .ranges()
                .get(index)
                .is_some_and(|range| range.start() <= *key)
            {
                exceptions.extend(values.iter().map(|ch| ClassUnicodeRange::new(*ch, *ch)));
            }
        }
        canonical.union(&ClassUnicode::new(exceptions));
        self.map_class(&canonical, true)
    }

    fn map_class(&self, input: &ClassUnicode, inverse: bool) -> ClassUnicode {
        let domains = ClassUnicode::new(self.lower.iter().map(|run| {
            ClassUnicodeRange::new(
                char::from_u32(run.range.start).expect("stored scalar start"),
                char::from_u32(run.range.end).expect("stored scalar end"),
            )
        }));
        let mut identity = input.clone();
        identity.difference(&domains);
        let mut result = identity.ranges().to_vec();
        for run in &self.lower {
            let (offset, delta) = if inverse {
                (run.delta, -run.delta)
            } else {
                (0, run.delta)
            };
            let start = (run.range.start as i32 + offset) as u32;
            let end = (run.range.end as i32 + offset) as u32;
            let first = input
                .ranges()
                .partition_point(|range| (range.end() as u32) < start);
            for range in &input.ranges()[first..] {
                if range.start() as u32 > end {
                    break;
                }
                let lower = (range.start() as u32).max(start) as i32 + delta;
                let upper = (range.end() as u32).min(end) as i32 + delta;
                result.push(ClassUnicodeRange::new(
                    char::from_u32(lower as u32).expect("mapped scalar start"),
                    char::from_u32(upper as u32).expect("mapped scalar end"),
                ));
            }
        }
        ClassUnicode::new(result)
    }

    pub fn from_scalars(
        mut classify: impl FnMut(char) -> RegexClasses,
        mut lowercase: impl FnMut(char) -> char,
        extras: Vec<(char, Vec<char>)>,
        empty_nonboundary: bool,
    ) -> Self {
        let mut classes: [Vec<Range>; 3] = std::array::from_fn(|_| Vec::new());
        let mut lower: Vec<LowerRun> = Vec::new();
        for scalar in 0..=0x10ffff {
            let Some(ch) = char::from_u32(scalar) else {
                continue;
            };
            for (ranges, member) in classes.iter_mut().zip(classify(ch).0) {
                if member {
                    if let Some(last) = ranges.last_mut().filter(|last| last.end + 1 == scalar) {
                        last.end = scalar;
                    } else {
                        ranges.push(Range {
                            start: scalar,
                            end: scalar,
                        });
                    }
                }
            }
            let mapped = lowercase(ch) as u32;
            if mapped != scalar {
                let delta = mapped as i32 - scalar as i32;
                if let Some(last) = lower
                    .last_mut()
                    .filter(|last| last.range.end + 1 == scalar && last.delta == delta)
                {
                    last.range.end = scalar;
                } else {
                    lower.push(LowerRun {
                        range: Range {
                            start: scalar,
                            end: scalar,
                        },
                        delta,
                    });
                }
            }
        }
        let mut extras: Vec<_> = extras
            .into_iter()
            .map(|(ch, mut values)| {
                values.sort_unstable();
                values.dedup();
                (ch, values.into_boxed_slice())
            })
            .collect();
        extras.sort_unstable_by_key(|(ch, _)| *ch);
        assert!(
            extras.windows(2).all(|pair| pair[0].0 != pair[1].0),
            "Duplicate regex case exception key"
        );
        Self {
            classes: classes.map(Vec::into_boxed_slice),
            lower: lower.into_boxed_slice(),
            extras: extras.into_boxed_slice(),
            empty_nonboundary,
        }
    }

    pub fn contains(&self, class: RegexClass, ch: char) -> bool {
        let ranges = self.class_ranges(class);
        let scalar = ch as u32;
        let index = ranges.partition_point(|range| range.end < scalar);
        ranges.get(index).is_some_and(|range| range.start <= scalar)
    }

    pub fn simple_lower(&self, ch: char) -> char {
        let scalar = ch as u32;
        let index = self.lower.partition_point(|run| run.range.end < scalar);
        self.lower
            .get(index)
            .filter(|run| run.range.start <= scalar)
            .map_or(ch, |run| {
                char::from_u32((scalar as i32 + run.delta) as u32)
                    .expect("Host lowercase mapping must remain a Unicode scalar")
            })
    }

    pub fn extra_cases(&self, ch: char) -> &[char] {
        self.extras
            .binary_search_by_key(&ch, |(key, _)| *key)
            .map_or(&[], |index| &self.extras[index].1)
    }

    pub fn empty_nonboundary(&self) -> bool {
        self.empty_nonboundary
    }

    /// Heap payload retained by the compressed policy, excluding this struct.
    pub fn allocated_bytes(&self) -> usize {
        self.classes
            .iter()
            .map(|ranges| std::mem::size_of_val(&**ranges))
            .sum::<usize>()
            + std::mem::size_of_val(&*self.lower)
            + std::mem::size_of_val(&*self.extras)
            + self
                .extras
                .iter()
                .map(|(_, values)| std::mem::size_of_val(&**values))
                .sum::<usize>()
    }
}
