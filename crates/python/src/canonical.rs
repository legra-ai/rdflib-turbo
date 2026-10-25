//! Immutable exact shapes input; no RDF text or native indexes on a cache hit.
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use pyo3::basic::CompareOp;
use pyo3::exceptions::PyValueError;
use pyo3::{IntoPyObjectExt, prelude::*};
use pyshacl_turbo_engine::oxigraph::{model::*, store::Store};
use pyshacl_turbo_engine::{Error, IndexedShapesSource, Result};

use crate::shape_terms::{self, DecodedShapes};
use crate::terms::Batch;

#[path = "canonical_arena.rs"]
mod arena;
#[cfg(test)]
#[path = "canonical_tests.rs"]
mod tests;

pub(crate) const BATCH_SIZE: usize = 256;

/// An exact immutable shapes source containing no Python owners or callbacks.
#[pyclass(frozen)]
pub(crate) struct ShapesInput {
    records: arena::Records,
    dataset: bool,
    hash: u64,
}

impl ShapesInput {
    fn from_records(records: arena::Records, dataset: bool) -> Self {
        let mut hash = DefaultHasher::new();
        records.hash(&mut hash);
        dataset.hash(&mut hash);
        Self {
            records,
            dataset,
            hash: hash.finish(),
        }
    }

    pub(crate) fn source(&self) -> Result<IndexedShapesSource> {
        // Only the compiler calls this on a cache miss. Input validation and
        // equality never allocate a Store, Dataset or native index.
        let store = Store::new().map_err(|error| Error::Runtime(error.to_string()))?;
        let mut transaction = store
            .start_transaction()
            .map_err(|error| Error::Runtime(error.to_string()))?;
        for record in self.records.iter() {
            match record {
                arena::Record::Quad(quad) => transaction.insert_generalized(quad),
                arena::Record::NamedGraph(graph) => transaction.insert_named_graph(graph),
            }
        }
        transaction
            .commit()
            .map_err(|error| Error::Runtime(error.to_string()))?;
        Ok(IndexedShapesSource::new(
            store.read_snapshot(),
            (!self.dataset).then_some(GraphName::DefaultGraph),
        ))
    }
}

#[pymethods]
impl ShapesInput {
    #[new]
    #[pyo3(signature = (source, dataset=false, named_graphs=None))]
    fn new(
        py: Python<'_>,
        source: &Bound<'_, PyAny>,
        dataset: bool,
        named_graphs: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let mut builder = arena::Builder::default();
        for batch in source.try_iter()? {
            let batch = batch?;
            if batch.get_item(0)?.len()? > 4 * BATCH_SIZE || batch.get_item(1)?.len()? > BATCH_SIZE
            {
                return Err(PyValueError::new_err(
                    "Shape transfer batches may contain at most 256 quads",
                ));
            }
            let input: Batch = batch.extract()?;
            py.detach(|| {
                let input = DecodedShapes::new(input)?;
                for quad in input.quads() {
                    let quad = quad?;
                    match quad.graph_name {
                        GraphNameRef::DefaultGraph => (),
                        graph if !dataset => {
                            return Err(Error::Invalid(format!(
                                "Selected shapes input cannot contain named context {graph}"
                            )));
                        }
                        GraphNameRef::NamedNode(node) => builder.push_graph(node.into()),
                        GraphNameRef::BlankNode(node) => builder.push_graph(node.into()),
                    }
                    builder.push_quad(quad);
                }
                builder.finish_batch();
                Ok::<_, Error>(())
            })
            .map_err(crate::error)?;
        }
        if let Some(graphs) = named_graphs {
            for record in graphs.try_iter()? {
                let record: shape_terms::Record = record?.extract()?;
                py.detach(|| {
                    let graph = shape_terms::term(record)?;
                    if !dataset {
                        return Err(Error::Invalid(
                            "Selected shapes input cannot declare named contexts".into(),
                        ));
                    }
                    match graph.as_ref() {
                        TermRef::NamedNode(node) => builder.push_graph(node.into()),
                        TermRef::BlankNode(node) => builder.push_graph(node.into()),
                        TermRef::Literal(_) => {
                            return Err(Error::Invalid(
                                "Shapes graph names must be IRIs or blank nodes".into(),
                            ));
                        }
                    }
                    Ok::<_, Error>(())
                })
                .map_err(crate::error)?;
            }
        }
        Ok(py.detach(|| Self::from_records(builder.finish(), dataset)))
    }

    fn __hash__(&self) -> u64 {
        self.hash
    }

    fn __len__(&self) -> usize {
        self.records.len()
    }

    fn __richcmp__(
        &self,
        py: Python<'_>,
        other: &Bound<'_, PyAny>,
        operation: CompareOp,
    ) -> PyResult<Py<PyAny>> {
        let Ok(other) = other.extract::<PyRef<'_, Self>>() else {
            return Ok(py.NotImplemented());
        };
        let other = &*other;
        let equal = || self.dataset == other.dataset && self.records == other.records;
        match operation {
            CompareOp::Eq => py.detach(equal).into_py_any(py),
            CompareOp::Ne => py.detach(|| !equal()).into_py_any(py),
            _ => Ok(py.NotImplemented()),
        }
    }
}
