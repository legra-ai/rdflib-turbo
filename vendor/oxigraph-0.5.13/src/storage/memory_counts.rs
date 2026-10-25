//! Committed cardinality publication and transaction-local membership changes.
use super::numeric_encoder::EncodedTerm;
use rustc_hash::FxHashMap;

#[derive(Default)]
pub(super) struct PublishedCounts {
    pub(super) version: usize,
    pub(super) total: usize,
    pub(super) default_count: usize,
    graphs: FxHashMap<EncodedTerm, usize>,
}

impl PublishedCounts {
    pub(super) fn graph_len(&self, graph: &EncodedTerm) -> usize {
        if matches!(graph, EncodedTerm::DefaultGraph) {
            self.default_count
        } else {
            self.graphs.get(graph).copied().unwrap_or(0)
        }
    }

    pub(super) fn publish(&mut self, pending: PendingCounts, version: usize) {
        for (graph, count) in pending.first.into_iter().chain(pending.graphs) {
            if count == 0 {
                self.graphs.remove(&graph);
            } else {
                self.graphs.insert(graph, count);
            }
        }
        self.default_count = pending.default_count;
        self.total = pending.total;
        self.version = version;
    }
}

pub(super) struct PendingCounts {
    pub(super) total: usize,
    default_count: usize,
    first: Option<(EncodedTerm, usize)>,
    graphs: FxHashMap<EncodedTerm, usize>,
}

impl PendingCounts {
    pub(super) fn new(total: usize, default_count: usize) -> Self {
        Self {
            total,
            default_count,
            first: None,
            graphs: FxHashMap::default(),
        }
    }

    pub(super) fn graph_len(&self, graph: &EncodedTerm, baseline: impl FnOnce() -> usize) -> usize {
        if matches!(graph, EncodedTerm::DefaultGraph) {
            return self.default_count;
        }
        if let Some((first, count)) = &self.first {
            if first == graph {
                return *count;
            }
        }
        self.graphs.get(graph).copied().unwrap_or_else(baseline)
    }

    fn named_count(&mut self, graph: &EncodedTerm, baseline: impl FnOnce() -> usize) -> &mut usize {
        if self.first.is_none() {
            // bounded: one touched named graph owns one inline scalar/key pair.
            return &mut self.first.insert((graph.clone(), baseline())).1;
        }
        let (first, count) = self.first.as_mut().unwrap();
        if first == graph {
            count
        } else {
            // Native metadata spills only for additional transaction-touched named graphs.
            self.graphs.entry(graph.clone()).or_insert_with(baseline)
        }
    }

    pub(super) fn change(
        &mut self,
        graph: &EncodedTerm,
        added: bool,
        baseline: impl FnOnce() -> usize,
    ) {
        // Default cardinality was captured with the committed version at transaction start.
        // It needs neither a repeated publication lock nor a hashed graph key.
        let graph_count = if matches!(graph, EncodedTerm::DefaultGraph) {
            &mut self.default_count
        } else {
            self.named_count(graph, baseline)
        };
        if added {
            *graph_count = graph_count
                .checked_add(1)
                .expect("graph cardinality overflow");
            self.total = self
                .total
                .checked_add(1)
                .expect("quad cardinality overflow");
        } else {
            *graph_count = graph_count
                .checked_sub(1)
                .expect("graph cardinality underflow");
            self.total = self
                .total
                .checked_sub(1)
                .expect("quad cardinality underflow");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::numeric_encoder::StrHash;
    use super::*;
    use std::cell::Cell;
    use std::sync::RwLock;

    #[test]
    fn default_count_publication_does_not_hash_a_named_graph_key() {
        let published = RwLock::new(PublishedCounts::default());
        let mut pending = PendingCounts::new(0, 0);
        pending.change(&EncodedTerm::DefaultGraph, true, || {
            published
                .read()
                .unwrap()
                .graph_len(&EncodedTerm::DefaultGraph)
        });
        let mut published = published.write().unwrap();
        published.publish(pending, 1);
        assert_eq!(published.graph_len(&EncodedTerm::DefaultGraph), 1);
        assert_eq!(published.total, 1);
        assert!(
            published.graphs.is_empty(),
            "default count must be an owned scalar"
        );
    }

    #[test]
    fn captured_default_count_never_reads_published_baseline() {
        let mut pending = PendingCounts::new(9, 4);
        let graph = EncodedTerm::DefaultGraph;
        let unexpected_read = || panic!("default count must use the transaction-start scalar");
        assert_eq!(pending.graph_len(&graph, unexpected_read), 4);
        pending.change(&graph, true, unexpected_read);
        assert_eq!(pending.graph_len(&graph, unexpected_read), 5);
        pending.change(&graph, false, unexpected_read);
        pending.change(&graph, false, unexpected_read);
        assert_eq!(pending.graph_len(&graph, unexpected_read), 3);
        assert_eq!(pending.total, 8);
        assert!(pending.first.is_none());
        assert!(pending.graphs.is_empty());
    }

    #[test]
    fn named_counts_read_baseline_only_once_per_touched_graph() {
        let first = EncodedTerm::NamedNode {
            iri_id: StrHash::new("urn:first"),
        };
        let second = EncodedTerm::NamedNode {
            iri_id: StrHash::new("urn:second"),
        };
        let reads = Cell::new(0);
        let baseline = || {
            reads.set(reads.get() + 1);
            2
        };
        let unexpected_read = || panic!("working named count must be retained");
        let mut pending = PendingCounts::new(7, 3);
        pending.change(&first, true, baseline);
        pending.change(&first, false, unexpected_read);
        pending.change(&second, false, baseline);
        pending.change(&second, true, unexpected_read);
        pending.change(&EncodedTerm::DefaultGraph, false, unexpected_read);
        assert_eq!(reads.get(), 2);
        assert_eq!(pending.graph_len(&first, unexpected_read), 2);
        assert_eq!(pending.graph_len(&second, unexpected_read), 2);
        assert_eq!(
            pending.graph_len(&EncodedTerm::DefaultGraph, unexpected_read),
            2
        );
        let mut published = PublishedCounts::default();
        published.publish(pending, 11);
        assert_eq!(published.total, 6);
        assert_eq!(published.version, 11);
        assert_eq!(published.graph_len(&EncodedTerm::DefaultGraph), 2);
        assert_eq!(published.graph_len(&first), 2);
        assert_eq!(published.graph_len(&second), 2);
        assert_eq!(published.graphs.len(), 2);
    }
}
