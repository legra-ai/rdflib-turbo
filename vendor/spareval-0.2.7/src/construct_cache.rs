//! Exact query-local triple identities retained by CONSTRUCT.

use rustc_hash::FxHashSet;

// On 32-bit targets usize already has the compact width.
#[cfg(not(target_pointer_width = "64"))]
pub(crate) type ConstructCache = FxHashSet<[usize; 3]>;

/// Disjoint exact sets avoid widening every common key for exceptional large IDs.
#[cfg(target_pointer_width = "64")]
#[derive(Default)]
pub(crate) struct ConstructCache {
    compact: FxHashSet<[u32; 3]>,
    wide: FxHashSet<[usize; 3]>,
}

#[cfg(target_pointer_width = "64")]
impl ConstructCache {
    fn compact_key(key: &[usize; 3]) -> Option<[u32; 3]> {
        let [Ok(subject), Ok(predicate), Ok(object)] = key.map(u32::try_from) else {
            return None;
        };
        Some([subject, predicate, object])
    }

    pub(crate) fn contains(&self, key: &[usize; 3]) -> bool {
        match Self::compact_key(key) {
            Some(key) => self.compact.contains(&key),
            None => self.wide.contains(key),
        }
    }

    pub(crate) fn insert(&mut self, key: [usize; 3]) -> bool {
        match Self::compact_key(&key) {
            Some(key) => self.compact.insert(key),
            None => self.wide.insert(key),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.compact.len() + self.wide.len()
    }

    pub(crate) fn clear(&mut self) {
        self.compact.clear();
        self.wide.clear();
    }
}

#[cfg(test)]
mod tests;
