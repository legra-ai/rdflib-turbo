//! One active invocation's checked slots, never an input-wide term cache.
use super::{
    Acquisition,
    caller::{Caller, Frames},
    live, source,
};
use pyo3::{exceptions::PyValueError, prelude::*, types::PySet};
use pyshacl_turbo_engine::oxigraph::model::*;
use pyshacl_turbo_engine::{
    Error, ReportTermRole, Result, SelectedLiteral, SelectedTerm, SelectedValueFrame,
};
use std::sync::{Arc, Weak};

pub(super) struct Frame {
    pub caller: Weak<Caller>,
    pub id: usize,
    pub issuer: Arc<()>,
}
impl std::fmt::Debug for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ActiveSelectedValueFrame")
    }
}
impl Frame {
    pub(super) fn caller(&self) -> Result<Arc<Caller>> {
        let caller = self
            .caller
            .upgrade()
            .ok_or_else(|| Error::Invalid("Selected caller has ended".into()))?;
        caller.check_thread()?;
        Ok(caller)
    }
    pub(super) fn index(&self, frames: &Frames) -> PyResult<usize> {
        frames
            .active
            .iter()
            .position(|f| f.id == self.id && Arc::ptr_eq(&f.issuer, &self.issuer))
            .ok_or_else(|| PyValueError::new_err("Selected frame is foreign or has ended"))
    }
    pub(super) fn object(
        &self,
        py: Python<'_>,
        caller: &Caller,
        slot: usize,
    ) -> PyResult<Py<PyAny>> {
        let frames = caller.frames.lock().expect("source frames lock poisoned");
        let i = self.index(&frames)?;
        let owner = frames.active[i]
            .payload
            .words()
            .get(slot)
            .ok_or_else(|| PyValueError::new_err("Selected value slot was not issued"))?;
        Ok(owner.literal.clone_ref(py))
    }
    pub(super) fn retain_count(&self, caller: &Caller, set: Bound<'_, PySet>) -> PyResult<usize> {
        let count = set.len();
        let mut frames = caller.frames.lock().expect("source frames lock poisoned");
        let i = self.index(&frames)?;
        frames.active[i].payload.counts_mut().push(set.unbind());
        Ok(count)
    }
    pub(super) fn stage_selected(
        &self,
        caller: &Caller,
        selected: &Bound<'_, PySet>,
        factories: &source::Factories<'_>,
    ) -> PyResult<Vec<SelectedTerm>> {
        let mut values = Vec::with_capacity(selected.len());
        let mut owners = Vec::new();
        for object in selected.iter() {
            let (term, _) = factories.stage(&object, None)?;
            let word = super::selection::word(term.as_ref());
            let local = word.map(|word| {
                let i = owners.len();
                owners.push((object.unbind(), word));
                i
            });
            values.push((term, local));
        }
        let mut frames = caller.frames.lock().expect("source frames lock poisoned");
        let i = self.index(&frames)?;
        let start = frames.active[i].payload.words().len();
        for (literal, word) in owners {
            let receipt = frames
                .next_receipt
                .checked_add(1)
                .ok_or_else(|| PyValueError::new_err("Selected receipt identity exhausted"))?;
            frames.next_receipt = receipt;
            frames.active[i].payload.words_mut().push(live::Acquired {
                literal,
                word,
                receipt,
            });
        }
        Ok(values
            .into_iter()
            .map(|(term, slot)| SelectedTerm {
                term,
                slot: slot.map(|s| s + start),
            })
            .collect())
    }
}
impl SelectedValueFrame for Frame {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn begin_nested(&self) -> Result<Arc<dyn SelectedValueFrame>> {
        pyshacl_turbo_engine::SelectedValueSource::begin_frame(self.caller()?.as_ref())
    }
    fn predicate(
        &self,
        focus: &Term,
        predicate: &NamedNode,
        subject: Option<pyshacl_turbo_engine::SelectedSubjectRef<'_>>,
    ) -> Result<Vec<SelectedTerm>> {
        let caller = self.caller()?;
        // A nested bound subject is the actual parent acquisition. Validate
        // issuer provenance before Python access; never reconstruct it by word.
        let inherited = super::subject::Issued::resolve(&caller, subject)?;
        caller.event(|py| {
            {
                let frames = caller.frames.lock().expect("source frames lock poisoned");
                self.index(&frames)?;
            }
            let factories = source::Factories::bind(py, &caller.classes);
            let s = if let Some(subject) = inherited {
                subject.object(py, &caller)?.into_bound(py)
            } else {
                factories.python_term(focus.as_ref())?
            };
            let p = factories.python_term(predicate.as_ref().into())?;
            let source = caller.graph.bind(py);
            let rows = match caller.acquisition {
                Acquisition::LegacyUnion => source
                    .getattr("store")?
                    .call_method1("triples", ((s, p, py.None()), py.None()))?,
                Acquisition::GraphObjects | Acquisition::PreparedDatasetObjects => {
                    source.call_method1("objects", (s, p))?
                }
            };
            // This is the actual reference value-node set, local to one demanded
            // SP acquisition. Its hash/equality events and order are authority.
            let selected = PySet::empty(py)?;
            for row in rows.try_iter()? {
                let row = row?;
                let object = match caller.acquisition {
                    Acquisition::LegacyUnion => row.get_item(0)?.get_item(2)?,
                    Acquisition::GraphObjects | Acquisition::PreparedDatasetObjects => row,
                };
                selected.add(object)?;
            }
            self.stage_selected(&caller, &selected, &factories)
        })
    }
    fn sequence(
        &self,
        focus: &Term,
        path: pyshacl_turbo_engine::SelectedPathRef<'_>,
        subject: Option<pyshacl_turbo_engine::SelectedSubjectRef<'_>>,
    ) -> Result<Vec<SelectedTerm>> {
        super::sequence::acquire(self, focus, path, subject)
    }
    fn cardinality(
        &self,
        focus: &Term,
        path: pyshacl_turbo_engine::SelectedPathRef<'_>,
        subject: Option<pyshacl_turbo_engine::SelectedSubjectRef<'_>>,
    ) -> Result<usize> {
        super::sequence::cardinality(self, focus, path, subject)
    }
    fn datatype(&self, slot: usize, expected: &NamedNode) -> Result<bool> {
        let caller = self.caller()?;
        caller.event(|py| {
            let object = self.object(py, &caller, slot)?;
            let object = object.bind(py);
            super::assessment::datatype(py, &caller, object, expected)
        })
    }
    fn retain(&self, slot: usize) -> Result<SelectedLiteral> {
        let caller = self.caller()?;
        Python::attach(|py| {
            caller
                .check_interpreter(py)
                .map_err(|e| Error::Invalid(e.to_string()))?;
            let frames = caller.frames.lock().expect("source frames lock poisoned");
            let i = self
                .index(&frames)
                .map_err(|e| Error::Invalid(e.to_string()))?;
            let owner = frames.active[i]
                .payload
                .words()
                .get(slot)
                .ok_or_else(|| Error::Invalid("Selected value slot was not issued".into()))?;
            Ok(SelectedLiteral(Arc::new(live::Owner {
                literal: owner.literal.clone_ref(py),
                issuer: caller.issuer.clone(),
                word: owner.word,
                receipt: owner.receipt,
            })))
        })
    }
    fn present(&self, slot: usize, _role: ReportTermRole) -> Result<String> {
        let caller = self.caller()?;
        caller.event(|py| {
            let object = self.object(py, &caller, slot)?;
            // The helper performs the actual formatter operations before the
            // scalar-only transport check. It is not a codec/cache snapshot.
            let text = py
                .import("rdflib_turbo._literal")?
                .getattr("_selected_presentation")?
                .call1((caller.graph.bind(py), object))?;
            Ok(text.cast::<pyo3::types::PyString>()?.to_str()?.to_owned())
        })
    }
    fn close(&self) {
        if let Some(caller) = self.caller.upgrade() {
            caller.close(self.id, &self.issuer);
        }
    }
}
