//! Adapt a pinned committed MVCC read view to the shared engine.
use oxigraph::model::*;
use oxigraph::store::ReadSnapshot;
use pyshacl_turbo_engine::{Error, GraphRead, Result};

pub(super) struct CommittedGraph(pub ReadSnapshot);

impl GraphRead for CommittedGraph {
    fn matching_quads<'a>(
        &'a self,
        subject: Option<NamedOrBlankNodeRef<'_>>,
        predicate: Option<NamedNodeRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = Result<Quad>> + use<'a> {
        self.0
            .quads_for_pattern(subject, predicate, object, graph)
            .map(|quad| quad.map_err(|error| Error::Runtime(error.to_string())))
    }

    fn named_graphs(&self) -> impl Iterator<Item = Result<NamedOrBlankNode>> + '_ {
        self.0
            .named_graphs()
            .map(|graph| graph.map_err(|error| Error::Runtime(error.to_string())))
    }

    fn contains_named_graph(&self, graph: &NamedOrBlankNode) -> Result<bool> {
        self.0
            .contains_named_graph(graph.as_ref())
            .map_err(|error| Error::Runtime(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxigraph::store::Store;

    #[test]
    fn committed_view_pins_one_version_across_writer_commit() {
        let store = Store::new().unwrap();
        let source = CommittedGraph(store.read_snapshot());
        let name = NamedNode::new("urn:graph").unwrap();
        let node = NamedNode::new("urn:node").unwrap();
        let mut transaction = store.start_transaction().unwrap();
        transaction.insert_named_graph(&name);
        transaction.insert(&Quad::new(node.clone(), node.clone(), node, name.clone()));
        assert_eq!(source.matching_quads(None, None, None, None).count(), 0);
        transaction.commit().unwrap();
        assert_eq!(source.matching_quads(None, None, None, None).count(), 0);
        assert!(!source.contains_named_graph(&name.clone().into()).unwrap());
        let current = CommittedGraph(store.read_snapshot());
        assert_eq!(current.matching_quads(None, None, None, None).count(), 1);
        assert!(current.contains_named_graph(&name.into()).unwrap());
    }
}
