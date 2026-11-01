//! Sealed, single-use report receipts resolve actual owners without property reads.
use crate::{
    boolean_input::live::{Issuer, Reported},
    terms::Batch,
};
use pyo3::{
    class::gc::{PyTraverseError, PyVisit},
    exceptions::PyValueError,
    prelude::*,
};
use pyshacl_turbo_engine::{BooleanObservation, Error, Result, SelectedLiteral};
use std::{collections::HashSet, sync::Arc};

type SelectedReportParts = (Batch, Vec<(usize, u8)>, Vec<(usize, Py<PyAny>)>);

struct Payload {
    batch: Batch,
    attested: Vec<(usize, u8)>,
    selected: Vec<(usize, SelectedLiteral)>,
    issuer: Arc<Issuer>,
}
/// Native-only construction; no caller-supplied receipt or Python cache snapshot.
#[pyclass(name = "SelectedLiteralReport")]
pub(crate) struct SelectedReport {
    payload: Option<Payload>,
}
impl SelectedReport {
    pub fn new(batch: Batch, entries: Vec<(usize, BooleanObservation)>) -> Result<Self> {
        let mut attested = Vec::new();
        let mut selected = Vec::new();
        let mut issuer = None;
        let mut seen = HashSet::new();
        let results: HashSet<_> = batch
            .1
            .iter()
            .filter_map(|&(s, p, o, g)| {
                let iri = |i: usize, name: &str| {
                    batch
                        .0
                        .get(i)
                        .is_some_and(|r| r.0 == 0 && r.1 == name && r.2.is_none() && r.3.is_none())
                };
                (iri(p, "http://www.w3.org/1999/02/22-rdf-syntax-ns#type")
                    && iri(o, "http://www.w3.org/ns/shacl#ValidationResult"))
                .then_some((s, g))
            })
            .collect();
        for (index, entry) in entries {
            if !seen.insert(index) {
                return Err(Error::Invalid(
                    "Duplicate selected report occurrence".into(),
                ));
            }
            match entry {
                BooleanObservation::Attested(state) => attested.push((index, state.code())),
                BooleanObservation::Reported(owner) => {
                    let native = Reported::checked(&owner)?;
                    if native.receipt() == 0 {
                        return Err(Error::Invalid("Unissued selected report receipt".into()));
                    }
                    if let Some(issuer) = &issuer {
                        if !Arc::ptr_eq(issuer, native.issuer()) {
                            return Err(Error::Invalid(
                                "Selected report mixes caller issuers".into(),
                            ));
                        }
                    } else {
                        issuer = Some(native.issuer().clone());
                    }
                    Self::check_word(&batch, index, native, Some(&results))?;
                    selected.push((index, owner));
                }
                BooleanObservation::Selected(_) | BooleanObservation::Focus(_) => {
                    return Err(Error::Invalid("Unpromoted selected report lease".into()));
                }
            }
        }
        let issuer =
            issuer.ok_or_else(|| Error::Invalid("Selected report has no live issuer".into()))?;
        Ok(Self {
            payload: Some(Payload {
                batch,
                attested,
                selected,
                issuer,
            }),
        })
    }
    fn check_word(
        batch: &Batch,
        index: usize,
        owner: Reported<'_>,
        results: Option<&HashSet<(usize, Option<usize>)>>,
    ) -> Result<()> {
        let (records, quads) = batch;
        let &(s, p, o, g) = quads
            .get(index)
            .ok_or_else(|| Error::Invalid("Invalid selected report quad index".into()))?;
        let iri = |i: usize, name: &str| {
            records
                .get(i)
                .is_some_and(|r| r.0 == 0 && r.1 == name && r.2.is_none() && r.3.is_none())
        };
        let value = records
            .get(o)
            .ok_or_else(|| Error::Invalid("Invalid selected report term index".into()))?;
        if !(iri(p, "http://www.w3.org/ns/shacl#value")
            || iri(p, "http://www.w3.org/ns/shacl#focusNode"))
            || !owner.matches(value.0, &value.1, value.2.as_deref(), value.3.as_deref())
            || results.is_some_and(|results| !results.contains(&(s, g)))
        {
            return Err(Error::Invalid(
                "Selected receipt is not its admitted direct SH.value/SH.focusNode word".into(),
            ));
        }
        Ok(())
    }
}
#[pymethods]
impl SelectedReport {
    fn __traverse__(&self, visit: PyVisit<'_>) -> std::result::Result<(), PyTraverseError> {
        if let Some(payload) = &self.payload {
            for (_, owner) in &payload.selected {
                let owner = Reported::checked(owner).expect("checked selected owner");
                visit.call(owner.literal())?;
            }
        }
        Ok(())
    }
    fn __clear__(&mut self) {
        self.payload = None;
    }
    fn take_parts(&mut self, py: Python<'_>) -> PyResult<SelectedReportParts> {
        let current = self
            .payload
            .as_ref()
            .ok_or_else(|| PyValueError::new_err("Selected report was already consumed"))?;
        if crate::comparison::interpreter(py)? != current.issuer.interpreter {
            return Err(PyValueError::new_err(
                "Selected report belongs to another interpreter",
            ));
        }
        let payload = self.payload.take().expect("checked live report payload");
        let mut live = Vec::with_capacity(payload.selected.len());
        for (index, owner) in payload.selected {
            let native = Reported::checked(&owner).map_err(crate::error)?;
            if !Arc::ptr_eq(native.issuer(), &payload.issuer) {
                return Err(PyValueError::new_err(
                    "Selected receipt belongs to another caller",
                ));
            }
            // The sealed acquisition receipt already proves the word; no host getters.
            Self::check_word(&payload.batch, index, native, None).map_err(crate::error)?;
            live.push((index, native.literal().clone_ref(py)));
        }
        Ok((payload.batch, payload.attested, live))
    }
}
