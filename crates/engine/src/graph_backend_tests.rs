//! A transaction is a borrowed read source, never a copied Dataset.
use oxigraph::model::*;
use oxigraph::store::Store;

use crate::graph::{objects, subjects};
use crate::path::Path;
use crate::{Error, GraphRead, Result};

fn node(name: &str) -> NamedNode {
    NamedNode::new(format!("urn:{name}")).unwrap()
}

#[test]
fn paths_read_pending_writes_and_deletes_without_exposing_them_to_store_readers() {
    let store = Store::new().unwrap();
    let triple = Quad::new(node("a"), node("p"), node("b"), GraphName::DefaultGraph);
    store.insert(&triple).unwrap();
    let mut transaction = store.start_transaction().unwrap();
    transaction.remove(&triple);
    transaction.insert(&Quad::new(
        node("a"),
        node("p"),
        node("c"),
        GraphName::DefaultGraph,
    ));
    transaction.insert(&Quad::new(
        node("c"),
        node("p"),
        node("d"),
        GraphName::DefaultGraph,
    ));
    let path = Path::OneOrMore(Box::new(Path::Predicate(node("p"))));
    assert_eq!(
        path.values(&transaction, &node("a").into()).unwrap(),
        vec![node("c").into(), node("d").into()]
    );
    assert!(store.contains(&triple).unwrap());
    drop(transaction);
    assert_eq!(store.len().unwrap(), 1);
    assert!(store.contains(&triple).unwrap());
}

#[test]
fn indexed_reads_keep_default_and_named_graphs_distinct() {
    let store = Store::new().unwrap();
    let mut transaction = store.start_transaction().unwrap();
    for graph in [GraphName::DefaultGraph, node("g").into()] {
        transaction.insert(&Quad::new(node("a"), node("p"), node("b"), graph));
    }
    assert_eq!(
        objects(&transaction, &node("a").into(), &node("p")).unwrap(),
        vec![node("b").into()]
    );
    assert_eq!(
        subjects(&transaction, &node("p"), Some(&node("b").into())).unwrap(),
        vec![node("a").into()]
    );
    assert_eq!(
        transaction.matching_quads(None, None, None, None).count(),
        2
    );
    assert_eq!(
        transaction
            .matching_quads(None, None, None, Some(node("g").as_ref().into()))
            .count(),
        1
    );
    assert!(
        objects(
            &transaction,
            &Literal::new_simple_literal("subject").into(),
            &node("p")
        )
        .unwrap()
        .is_empty()
    );
}

struct FailingGraph;

impl GraphRead for FailingGraph {
    fn selected_focus_source(&self) -> Option<&dyn crate::SelectedFocusSource> {
        None
    }
    fn selected_value_source(&self) -> Option<&dyn crate::SelectedValueSource> {
        None
    }

    fn boolean_object_state(
        &self,
        _row: &oxigraph::model::GeneralizedQuad,
    ) -> crate::Result<Option<crate::BuiltinBooleanState>> {
        Ok(None)
    }

    fn matching_quads<'a>(
        &'a self,
        _: Option<TermRef<'_>>,
        _: Option<TermRef<'_>>,
        _: Option<TermRef<'_>>,
        _: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a> {
        std::iter::once(Err(Error::Runtime("storage read failed".into())))
    }
    fn named_graphs(&self) -> impl Iterator<Item = Result<NamedOrBlankNode>> + '_ {
        std::iter::once(Err(crate::Error::Runtime("graph metadata error".into())))
    }

    fn contains_named_graph(&self, _: &NamedOrBlankNode) -> Result<bool> {
        Err(Error::Runtime("storage read failed".into()))
    }
}

#[test]
fn traversal_propagates_storage_failures_instead_of_reporting_no_values() {
    let path = Path::Predicate(node("p"));
    assert!(
        path.values(&FailingGraph, &node("a").into())
            .unwrap_err()
            .to_string()
            .contains("storage read failed")
    );
    assert!(subjects(&FailingGraph, &node("p"), None).is_err());
    assert!(
        objects(
            &FailingGraph,
            &Literal::new_simple_literal("literal focus").into(),
            &node("p"),
        )
        .is_err()
    );
}
