//! One ordinary-input owner and sparse source assessment, never a Python term mirror.
use pyo3::class::gc::{PyTraverseError, PyVisit};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyType};
use pyshacl_turbo_engine::oxigraph::model::*;
use std::sync::Arc;
mod assessment;
mod borrowed;
pub(super) mod caller;
mod focus;
mod focus_acquire;
mod frame;
pub(super) mod live;
pub(super) mod selection;
mod sequence;
mod source;
mod subject;
const WINDOW: usize = 128;

/// Actual source acquisition authority, independent of its raw quad format.
#[derive(Clone, Copy, Debug)]
pub(super) enum Acquisition {
    GraphObjects,
    PreparedDatasetObjects,
    LegacyUnion,
}

/// Call-local ordinary source with one moved, immutable parsed Dataset.
#[pyclass(frozen)]
pub(crate) struct BooleanInput {
    pub data: Option<Arc<FrozenDataset>>,
    graph: Py<PyAny>,
    view: Option<Py<PyAny>>,
    classes: [Py<PyType>; 3],
    acquisition: Acquisition,
    pub(super) canonical_booleans: bool,
}
#[pymethods]
impl BooleanInput {
    fn __traverse__(&self, visit: PyVisit<'_>) -> std::result::Result<(), PyTraverseError> {
        visit.call(&self.graph)?;
        if let Some(view) = &self.view {
            visit.call(view)?;
        }
        for class in &self.classes {
            visit.call(class)?;
        }
        Ok(())
    }
    #[new]
    #[pyo3(signature=(graph,nodes,flatten=false,*,_terms,_dataset,_view=None))]
    fn new(
        py: Python<'_>,
        graph: Py<PyAny>,
        nodes: &Bound<'_, PyDict>,
        flatten: bool,
        _terms: [Py<PyType>; 3],
        _dataset: Option<Py<PyType>>,
        _view: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let source = graph.bind(py);
        let context_aware = source.getattr("context_aware")?.is_truthy()?;
        let acquisition = if let Some(dataset) = _dataset {
            if !source.get_type().is_subclass(dataset.bind(py))? {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "Prepared Dataset acquisition requires an actual Dataset instance",
                ));
            }
            Acquisition::PreparedDatasetObjects
        } else if context_aware {
            Acquisition::LegacyUnion
        } else {
            Acquisition::GraphObjects
        };
        if let Some(view) = _view {
            if !matches!(acquisition, Acquisition::GraphObjects) || flatten {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "Borrowed DATA requires an ordinary non-context Graph",
                ));
            }
            source::Factories::new(py, &_terms)?;
            if !view.bind(py).getattr("impl")?.is(source)
                || view.bind(py).call_method0("is_multigraph")?.is_truthy()?
            {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "Borrowed source view must select the same ordinary Graph",
                ));
            }
            return Ok(Self {
                data: None,
                graph,
                view: Some(view),
                classes: _terms,
                acquisition,
                canonical_booleans: false,
            });
        }
        let (default, context_graph_class) = if context_aware {
            let rdflib = py.import("rdflib")?;
            // Dataset and its legacy base expose distinct default-graph APIs.
            // Select by the actual type, never by swallowing a getter failure.
            let attribute = if source.is_instance(&rdflib.getattr("ConjunctiveGraph")?)?
                && !source.is_instance(&rdflib.getattr("Dataset")?)?
            {
                "default_context"
            } else {
                "default_graph"
            };
            (
                Some(source.getattr(attribute)?.getattr("identifier")?),
                Some(rdflib.getattr("Graph")?),
            )
        } else {
            (None, None)
        };
        let rows = if context_aware {
            source.call_method0("quads")?
        } else {
            source.clone()
        };
        let mut rows = rows.try_iter()?;
        let factories = source::Factories::new(py, &_terms)?;
        let mut data = DatasetLeaseBuilder::new();
        let mut canonical_booleans = false;
        loop {
            // bounded: one transfer window contains at most WINDOW RDF rows.
            let mut batch = Vec::with_capacity(WINDOW);
            while batch.len() < WINDOW {
                let Some(row) = rows.next() else { break };
                batch.push(factories.quad(
                    &row?,
                    Some(nodes),
                    context_aware,
                    default.as_ref(),
                    context_graph_class.as_ref(),
                    flatten,
                )?);
            }
            if batch.is_empty() {
                break;
            }
            py.detach(|| {
                for row in batch {
                    // Candidate proof is RDF word/datatype presence, including opaque caches.
                    canonical_booleans |= row.boolean;
                    data.insert(row.quad.as_ref());
                }
            });
        }
        let data = data.freeze();
        Ok(Self {
            data: Some(Arc::new(data)),
            graph,
            view: None,
            classes: _terms,
            acquisition,
            canonical_booleans,
        })
    }
}

#[cfg(test)]
mod tests;
