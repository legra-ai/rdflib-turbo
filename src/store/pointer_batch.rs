//! Fixed-size identifier reuse while materializing one bounded batch.
use oxigraph::store::TermText;
use pyo3::prelude::*;

const ENTRIES: usize = 4;
const MAX_BYTES: usize = 256;
const DISABLE_MISSES: usize = 8;

struct Entry {
    blank: bool,
    text: TermText,
    value: Py<PyAny>,
}

pub(super) struct PositionCache {
    // bounded: four immutable <=256-byte identifier owners per position, one batch lifetime.
    entries: [Option<Entry>; ENTRIES],
    next: usize,
    misses: usize,
}

impl Default for PositionCache {
    fn default() -> Self {
        Self {
            entries: std::array::from_fn(|_| None),
            next: 0,
            misses: 0,
        }
    }
}

impl PositionCache {
    pub(super) fn active(&self) -> bool {
        self.misses < DISABLE_MISSES
    }

    pub(super) fn eligible(&self, text: &TermText) -> bool {
        self.active() && text.as_str().len() <= MAX_BYTES
    }

    pub(super) fn get(
        &mut self,
        py: Python<'_>,
        blank: bool,
        text: &TermText,
    ) -> Option<Py<PyAny>> {
        for entry in self.entries.iter().flatten() {
            if entry.blank == blank && std::ptr::eq(entry.text.as_str(), text.as_str()) {
                self.misses = 0;
                return Some(entry.value.clone_ref(py));
            }
        }
        self.misses += 1;
        if !self.active() {
            // Native scan locks have already been released; Python decrefs are safe here.
            self.entries = std::array::from_fn(|_| None);
        }
        None
    }

    pub(super) fn insert(
        &mut self,
        py: Python<'_>,
        blank: bool,
        text: TermText,
        value: &Py<PyAny>,
    ) {
        if self.eligible(&text) {
            self.entries[self.next] = Some(Entry {
                blank,
                text,
                value: value.clone_ref(py),
            });
            self.next = (self.next + 1) % ENTRIES;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxigraph::model::{GraphName, NamedNode, Quad};
    use oxigraph::store::{LexicalTerm, Store};

    fn texts() -> Vec<TermText> {
        let store = Store::new().unwrap();
        for index in 0..10 {
            store
                .insert(
                    Quad::new(
                        NamedNode::new_unchecked(format!("urn:subject:long:{index}")),
                        NamedNode::new_unchecked("urn:predicate:long"),
                        NamedNode::new_unchecked(format!("urn:object:long:{index}")),
                        GraphName::DefaultGraph,
                    )
                    .as_ref(),
                )
                .unwrap();
        }
        let (rows, _) =
            super::super::super::read::start(&store, None, None, None, None, 1).unwrap();
        rows.into_iter()
            .map(|(subject, _, _)| match subject.unwrap() {
                LexicalTerm::NamedNode(text) => text,
                _ => panic!("named node expected"),
            })
            .collect()
    }

    #[test]
    fn pointer_hits_kind_separation_and_fixed_miss_limit() {
        let texts = texts();
        Python::initialize();
        Python::attach(|py| {
            let mut cache = PositionCache::default();
            let value = py.None();
            assert!(cache.get(py, false, &texts[0]).is_none());
            cache.insert(py, false, texts[0].clone(), &value);
            assert!(cache.get(py, false, &texts[0]).is_some());
            assert!(cache.get(py, true, &texts[0]).is_none());
            for text in texts.iter().skip(1).take(6) {
                assert!(cache.get(py, false, text).is_none());
                cache.insert(py, false, text.clone(), &value);
            }
            assert_eq!(cache.entries.iter().flatten().count(), ENTRIES);
            assert!(cache.active());
            assert!(cache.get(py, false, &texts[8]).is_none());
            assert!(!cache.active());
            assert_eq!(cache.entries.iter().flatten().count(), 0);
            cache.insert(py, false, texts[8].clone(), &value);
            assert_eq!(cache.entries.iter().flatten().count(), 0);
        });
    }
}
