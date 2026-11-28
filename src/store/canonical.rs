//! Canonical N-Quads and canonical copies for native stores and transactions.
//!
//! Quads are collected on the owner (store snapshot or transaction session);
//! blank-node labelling runs afterwards, so an exhausted work budget is a
//! read failure that never aborts the caller's transaction.
use pyo3::prelude::*;

use super::native_store::NativeStore;
use super::operations::{Operation, Reply};
use super::session::Session;
use super::typed::{self, TermTuple};
use crate::canonical::{CanonicalForm, CanonicalRequest, CanonicalScope};
use crate::{Error, Result};

/// What one canonicalization returns to Python.
#[derive(Clone, Copy)]
enum CanonicalOutput {
    /// The sorted canonical N-Quads document.
    NQuads,
    /// An independent native store holding the relabelled quads.
    Store,
}

impl CanonicalOutput {
    fn parse(source: &str) -> Result<Self> {
        match source {
            "nquads" => Ok(Self::NQuads),
            "store" => Ok(Self::Store),
            other => Err(Error::Invalid(format!(
                "Unknown canonical output: {other:?}"
            ))),
        }
    }
}

/// A finished canonicalization, converted to Python after native locks drop.
pub(super) enum Canonical {
    NQuads(String),
    Store(NativeStore),
}

impl Canonical {
    pub(super) fn into_python(self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match self {
            Self::NQuads(text) => Ok(text.into_pyobject(py)?.into_any().unbind()),
            Self::Store(store) => Ok(Py::new(py, store)?.into_any()),
        }
    }
}

/// The Python arguments of one canonicalization call.
pub(super) struct CanonicalCall<'a> {
    dataset: bool,
    graph: Option<TermTuple>,
    hash_algorithm: &'a str,
    work_budget: u64,
    identity: &'a str,
    output: &'a str,
}

struct Parsed {
    scope: CanonicalScope,
    request: CanonicalRequest,
    output: CanonicalOutput,
}

impl Parsed {
    fn finish(self, form: &CanonicalForm) -> Result<Canonical> {
        match self.output {
            CanonicalOutput::NQuads => form.render(self.request).map(Canonical::NQuads),
            CanonicalOutput::Store => form
                .store(self.request)
                .map(|store| Canonical::Store(NativeStore::from_store(store))),
        }
    }
}

impl<'a> CanonicalCall<'a> {
    /// `scope` is `(whole dataset, graph name)`; `request` is
    /// `(hash algorithm, work budget, term identity, output)`.
    pub(super) fn new(
        (dataset, graph): (bool, Option<TermTuple>),
        (hash_algorithm, work_budget, identity, output): (&'a str, u64, &'a str, &'a str),
    ) -> Self {
        Self {
            dataset,
            graph,
            hash_algorithm,
            work_budget,
            identity,
            output,
        }
    }

    fn parse(self) -> Result<Parsed> {
        let request =
            CanonicalRequest::try_new(self.hash_algorithm, self.work_budget, self.identity)?;
        let output = CanonicalOutput::parse(self.output)?;
        let scope = if self.dataset {
            CanonicalScope::Dataset
        } else {
            CanonicalScope::Graph(typed::graph(self.graph)?)
        };
        Ok(Parsed {
            scope,
            request,
            output,
        })
    }

    pub(super) fn on_store(self, store: &NativeStore) -> Result<Canonical> {
        let parsed = self.parse()?;
        let snapshot = store.snapshot()?;
        let quads =
            snapshot.generalized_quads_for_pattern(None, None, None, parsed.scope.pattern());
        let form = CanonicalForm::collect(quads, &parsed.scope, parsed.request.identity())?;
        parsed.finish(&form)
    }

    pub(super) fn on_transaction(self, session: &Session) -> Result<Canonical> {
        let parsed = match self.parse() {
            Ok(parsed) => parsed,
            Err(cause) => {
                session.finish(false)?;
                return Err(cause);
            }
        };
        let operation = Operation::CanonicalForm(parsed.scope.clone(), parsed.request.identity());
        match session.execute(operation)? {
            Reply::Canonical(form) => parsed.finish(&form),
            _ => unreachable!("native transaction canonical reply type"),
        }
    }
}
