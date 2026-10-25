//! Independent native ownership for explicitly requested scoped graph copies.

use std::sync::{Arc, Mutex};

use oxigraph::model::GraphName;
use oxigraph::store::{Store, Transaction};
use pyshacl_turbo_engine::GraphRead;

use super::NativeStore;
use super::operations::{Reply, runtime};
use super::pattern;
use super::session::Control;
use crate::Result;

impl NativeStore {
    pub(super) fn from_store(store: Store) -> Self {
        Self {
            control: Arc::new(Mutex::new(Control::new(store))),
        }
    }

    pub(super) fn open(path: Option<String>) -> Result<Self> {
        let store = match path {
            None => Store::new().map_err(runtime)?,
            Some(path) => {
                #[cfg(feature = "persistence")]
                {
                    Store::open(path).map_err(runtime)?
                }
                #[cfg(not(feature = "persistence"))]
                {
                    let _ = path;
                    return Err(crate::Error::Unsupported(
                        "This build has no RocksDB persistence support".into(),
                    ));
                }
            }
        };
        Ok(Self::from_store(store))
    }

    pub(super) fn from_copy_reply(reply: Reply) -> Self {
        match reply {
            Reply::Store(store) => Self::from_store(store),
            _ => unreachable!("native fork reply type"),
        }
    }
}

pub(super) fn fork(source: &Transaction<'_>, graph: Option<String>) -> Result<Store> {
    let graph = graph.as_deref().map(pattern::graph).transpose()?;
    copy_graph(source, graph)
}

pub(super) fn copy_graph(
    source: &(impl GraphRead + ?Sized),
    graph: Option<GraphName>,
) -> Result<Store> {
    let copied = Store::new().map_err(runtime)?;
    let mut output = copied.start_transaction().map_err(runtime)?;
    for quad in source.matching_quads(None, None, None, graph.as_ref().map(GraphName::as_ref)) {
        output.insert_generalized(&quad?);
    }
    match graph {
        None => {
            for graph in source.named_graphs() {
                output.insert_named_graph(&graph?);
            }
        }
        Some(GraphName::NamedNode(graph)) => {
            if source.contains_named_graph(&graph.clone().into())? {
                output.insert_named_graph(&graph);
            }
        }
        Some(GraphName::BlankNode(graph)) => {
            if source.contains_named_graph(&graph.clone().into())? {
                output.insert_named_graph(&graph);
            }
        }
        Some(GraphName::DefaultGraph) => {}
    }
    output.commit().map_err(runtime)?;
    Ok(copied)
}

#[cfg(test)]
#[path = "fork_tests.rs"]
mod tests;
