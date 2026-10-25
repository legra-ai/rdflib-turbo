//! Bounded raw-term ingestion into one unpublished native owner.
use oxigraph::model::{GeneralizedQuadRef, GraphNameRef};
use oxigraph::store::Transaction;
use pyo3::prelude::*;

use super::NativeStore;
use super::input_terms::{NativeInputTerms, PinnedPattern, PinnedTerm};
use super::operations::Operation;
use super::session::Session;
use super::typed_view::{Position, PreparedTerm};
use crate::{Result, error};

// bounded: one rendezvous command owns at most 128 pinned input records.
const BATCH_SIZE: usize = 128;

pub(super) enum Batch {
    Records(Vec<PinnedPattern>),
    Graphs(Vec<Option<PinnedTerm>>),
}

impl Batch {
    pub(super) fn insert(self, transaction: &mut Transaction<'_>) -> Result<()> {
        match self {
            Self::Records(records) => {
                for record in records {
                    let subject = PreparedTerm::new(record.subject.view(), Position::Data)?;
                    let predicate = PreparedTerm::new(record.predicate.view(), Position::Data)?;
                    let object = PreparedTerm::new(record.object.view(), Position::Data)?;
                    let graph = record
                        .graph
                        .as_ref()
                        .map(|graph| PreparedTerm::new(graph.view(), Position::GraphName))
                        .transpose()?;
                    transaction.insert_generalized(GeneralizedQuadRef {
                        subject: subject.term_ref(),
                        predicate: predicate.term_ref(),
                        object: object.term_ref(),
                        graph_name: graph.as_ref().map_or(GraphNameRef::DefaultGraph, |graph| {
                            graph.graph_name_ref().into()
                        }),
                    });
                }
            }
            Self::Graphs(graphs) => {
                for graph in graphs.into_iter().flatten() {
                    let graph = PreparedTerm::new(graph.view(), Position::GraphName)?;
                    transaction.insert_named_graph(graph.graph_name_ref());
                }
            }
        }
        Ok(())
    }
}

pub(super) fn from_records(
    py: Python<'_>,
    records: &Bound<'_, PyAny>,
    declared_graphs: &Bound<'_, PyAny>,
    inputs: &NativeInputTerms,
) -> PyResult<NativeStore> {
    let owner = py.detach(|| NativeStore::open(None)).map_err(error)?;
    let session = py
        .detach(|| Session::start(owner.control.clone()))
        .map_err(error)?;
    let mut records = records.try_iter()?;
    loop {
        // bounded: only this fixed input window survives the detached native call.
        let mut batch = Vec::with_capacity(BATCH_SIZE);
        while batch.len() < BATCH_SIZE {
            let Some(record) = records.next() else { break };
            let record = record?;
            let raw = inputs.stage_record(py, &record)?;
            batch.push(raw.pin(py)?);
        }
        if batch.is_empty() {
            break;
        }
        py.detach(|| session.execute(Operation::ImportBatch(Batch::Records(batch))))
            .map_err(error)?;
    }
    let mut graphs = declared_graphs.try_iter()?;
    loop {
        // bounded: graph declarations use the same fixed-size import window.
        let mut batch = Vec::with_capacity(BATCH_SIZE);
        while batch.len() < BATCH_SIZE {
            let Some(graph) = graphs.next() else { break };
            batch.push(inputs.stage_graph(py, graph?)?);
        }
        if batch.is_empty() {
            break;
        }
        py.detach(|| session.execute(Operation::ImportBatch(Batch::Graphs(batch))))
            .map_err(error)?;
    }
    // Neither the owner nor any reader escapes before both iterators succeed.
    py.detach(|| session.finish(true)).map_err(error)?;
    Ok(owner)
}
