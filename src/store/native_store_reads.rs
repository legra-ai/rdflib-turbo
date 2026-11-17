//! Python-bound read operations of [`NativeStore`].

use pyo3::prelude::*;

use super::native_store::NativeStore;
use super::{context_read, input_terms, quad_read, read, read_input, term_output, union_read};
use crate::error;

impl NativeStore {
    pub(super) fn contains_term_tuples(
        &self,
        py: Python<'_>,
        subject: read_input::TermTuple,
        predicate: read_input::TermTuple,
        object: read_input::TermTuple,
        graph: Option<read_input::TermTuple>,
    ) -> PyResult<bool> {
        py.detach(|| {
            read::contains_exact(
                &self.snapshot()?,
                read_input::term_view(&subject),
                read_input::term_view(&predicate),
                read_input::term_view(&object),
                read_input::view(&graph),
            )
        })
        .map_err(error)
    }

    pub(super) fn contains_staged_pattern<'py>(
        &self,
        py: Python<'py>,
        pattern: &Bound<'py, PyAny>,
        context: &Bound<'py, PyAny>,
        inputs: Py<input_terms::NativeInputTerms>,
    ) -> PyResult<bool> {
        let staged = inputs.bind(py).get().stage(py, pattern, context)?;
        let pinned = staged.pin(py)?;
        py.detach(|| {
            read::contains_exact(
                &self.snapshot()?,
                pinned.subject.view(),
                pinned.predicate.view(),
                pinned.object.view(),
                pinned.graph.as_ref().map(input_terms::PinnedTerm::view),
            )
        })
        .map_err(error)
    }

    pub(super) fn matches_staged_pattern<'py>(
        &self,
        py: Python<'py>,
        pattern: &Bound<'py, PyAny>,
        context: &Bound<'py, PyAny>,
        inputs: Py<input_terms::NativeInputTerms>,
    ) -> PyResult<bool> {
        let staged = inputs.bind(py).get().stage(py, pattern, context)?;
        let pinned = staged.pin_read()?;
        py.detach(|| {
            read::contains_matching(
                &self.snapshot()?,
                pinned.subject.as_ref().map(input_terms::PinnedTerm::view),
                pinned.predicate.as_ref().map(input_terms::PinnedTerm::view),
                pinned.object.as_ref().map(input_terms::PinnedTerm::view),
                pinned.graph.as_ref().map(input_terms::PinnedTerm::view),
            )
        })
        .map_err(error)
    }

    pub(super) fn first_union_batch(
        &self,
        py: Python<'_>,
        pattern: read_input::Pattern,
        factories: Py<term_output::NativeTermFactories>,
        scope: Option<(String, Option<read_input::TermTuple>)>,
    ) -> PyResult<union_read::FirstBatch> {
        let stream = py
            .detach(|| union_read::open(&self.snapshot()?, &pattern, scope.as_ref()))
            .map_err(error)?;
        union_read::first(py, stream, factories)
    }

    pub(super) fn first_context_batch(
        &self,
        py: Python<'_>,
        pattern: read_input::Pattern,
        factories: Py<term_output::NativeTermFactories>,
    ) -> PyResult<context_read::FirstBatch> {
        let stream = py
            .detach(|| context_read::open(&self.snapshot()?, &pattern))
            .map_err(error)?;
        context_read::first(py, stream, factories)
    }

    pub(super) fn first_quad_batch(
        &self,
        py: Python<'_>,
        pattern: read_input::Pattern,
        scope: (String, Option<read_input::TermTuple>),
        factories: Py<term_output::NativeTermFactories>,
    ) -> PyResult<quad_read::FirstBatch> {
        let stream = py
            .detach(|| quad_read::open(&self.snapshot()?, &pattern, &scope))
            .map_err(error)?;
        quad_read::first(py, stream, factories)
    }

    pub(super) fn first_pattern_batch(
        &self,
        py: Python<'_>,
        pattern: read_input::Pattern,
        graph: Option<read_input::TermTuple>,
        mask: u8,
        factories: Py<term_output::NativeTermFactories>,
    ) -> PyResult<term_output::FirstBatch> {
        let (subject, predicate, object) = pattern;
        let batch = py
            .detach(|| {
                read::start(
                    &self.snapshot()?,
                    read_input::view(&subject),
                    read_input::view(&predicate),
                    read_input::view(&object),
                    read_input::view(&graph),
                    mask,
                )
            })
            .map_err(error)?;
        term_output::first(py, batch, factories)
    }
}
