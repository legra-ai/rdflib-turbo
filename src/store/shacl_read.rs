//! Adapt a pinned committed MVCC read view to the shared engine.
use oxigraph::model::*;
use oxigraph::store::ReadSnapshot;
use pyshacl_turbo_engine::{Error, GraphRead, Result};

pub(super) struct CommittedGraph(pub ReadSnapshot);

impl GraphRead for CommittedGraph {
    fn selected_focus_source(&self) -> Option<&dyn pyshacl_turbo_engine::SelectedFocusSource> {
        None
    }
    fn selected_value_source(&self) -> Option<&dyn pyshacl_turbo_engine::SelectedValueSource> {
        None
    }

    fn boolean_object_state(
        &self,
        _row: &oxigraph::model::GeneralizedQuad,
    ) -> pyshacl_turbo_engine::Result<Option<pyshacl_turbo_engine::BuiltinBooleanState>> {
        Ok(None)
    }

    fn matching_quads<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a> {
        self.0
            .generalized_quads_for_pattern(subject, predicate, object, graph)
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
    use std::sync::{Arc, Mutex};

    use super::super::{NativeStore, session::Control};
    use super::*;
    use oxigraph::store::Store;
    use pyshacl_turbo_engine::Options;

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

    #[test]
    fn native_shape_compiler_retains_selected_and_dataset_sources_after_owner_close() {
        pyo3::Python::initialize();
        pyo3::Python::attach(|py| {
            for (scope, context) in [
                (GraphName::DefaultGraph, ""),
                (
                    NamedNode::new("urn:scope").unwrap().into(),
                    r#"{"type":"uri","value":"urn:scope"}"#,
                ),
                (
                    BlankNode::new("scope").unwrap().into(),
                    r#"{"type":"bnode","value":"scope"}"#,
                ),
            ] {
                let store = Store::new().unwrap();
                let shape: Term = Literal::new_simple_literal("shape").into();
                let target = NamedNode::new("http://www.w3.org/ns/shacl#targetNode").unwrap();
                let class = NamedNode::new("http://www.w3.org/ns/shacl#class").unwrap();
                let focus = NamedNode::new("urn:focus").unwrap();
                let expected = NamedNode::new("urn:Expected").unwrap();
                let row =
                    GeneralizedQuad::new(shape.clone(), class.clone(), expected, scope.clone());
                store.insert_generalized(&row).unwrap();
                store
                    .insert_generalized(&GeneralizedQuad::new(shape, target.clone(), focus, scope))
                    .unwrap();
                store
                    .insert_generalized(&GeneralizedQuad::new(
                        NamedNode::new("urn:hidden").unwrap(),
                        target,
                        NamedNode::new("urn:hidden-focus").unwrap(),
                        NamedNode::new("urn:hidden-graph").unwrap(),
                    ))
                    .unwrap();
                let owner = NativeStore {
                    control: Arc::new(Mutex::new(Control::new(store.clone()))),
                };
                let selected = owner
                    .compile_shapes(
                        py,
                        false,
                        true,
                        vec![],
                        Some(context),
                        crate::native_tests::host_regex_policy(py),
                    )
                    .unwrap();
                let dataset = owner
                    .compile_shapes(
                        py,
                        false,
                        true,
                        vec![],
                        None,
                        crate::native_tests::host_regex_policy(py),
                    )
                    .unwrap();
                assert_eq!(selected.shared_program().shape_nodes().count(), 1);
                assert_eq!(dataset.shared_program().shape_nodes().count(), 2);
                store.remove_generalized(&row).unwrap();
                owner.close(py).unwrap();
                drop(store);
                assert!(
                    !selected
                        .shared_program()
                        .validate(&mut Dataset::new(), Options::default())
                        .unwrap()
                        .conforms
                );
            }
        });
    }
}
