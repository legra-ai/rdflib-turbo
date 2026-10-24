//! Changed-only streaming removal over the existing ordered index.
use super::*;
use std::ops::Bound::{Excluded, Unbounded};

impl GeneralizedTransaction<'_> {
    /// Removes matching rows without collecting or copying the base relation.
    ///
    /// The predicate sees every visited row in predicate/object/subject/graph order.
    /// Panicking predicates unwind through the transaction's original-record undo.
    pub fn remove_where(
        &mut self,
        mut predicate: impl FnMut(GeneralizedQuadRef<'_>) -> bool,
    ) -> usize {
        // bounded: one shared record resumes the ordered scan after each removal.
        let mut previous: Option<PosgKey> = None;
        let mut removed = 0;
        loop {
            let start = previous.as_ref().map_or(Unbounded, Excluded);
            let next = self
                .data
                .data
                .posg
                .range((start, Unbounded))
                .find(|key| predicate(self.data.decode(&key.record)))
                .cloned();
            let Some(next) = next else {
                return removed;
            };
            let record = self
                .data
                .data
                .take_record(&next.record)
                .expect("selected generalized record must remain present");
            previous = Some(next);
            self.journal.removed(record);
            removed += 1;
        }
    }
}
