//! Legacy Dataset borrows one read lease; only derived output survives it.
use oxigraph::model::NamedNode;
use pyo3::prelude::*;

use super::commit::{Pending, Prepared};
use super::settings::{Captured, Settings};
use super::{Output, Report, SharedShapes};
use crate::NativeDataset;
use crate::inference::BorrowedInference;

impl SharedShapes {
    pub(super) fn prepare_inferred_dataset(
        &self,
        py: Python<'_>,
        dataset: &NativeDataset,
        settings: &str,
        inplace: bool,
    ) -> PyResult<Option<(Output, Option<Pending>)>> {
        let Captured {
            mut options,
            inference,
            publisher,
        } = Settings::capture(py, settings)?;
        let Some(inference) = inference else {
            return Ok(None);
        };
        let result = py.detach(|| {
            let data = dataset
                .inner
                .read()
                .map_err(|_| crate::Error::Runtime("Dataset lock poisoned".into()))?;
            if inplace {
                data.next_revision()?;
            }
            options.namespaces = data.namespaces.clone();
            let mut source = BorrowedInference::new(
                &data.data,
                NamedNode::new_unchecked("urn:pyshacl:inference").into(),
            );
            let outcome = source.expand(inference.mode.name(), inference.request)?;
            outcome.bind_namespace(&mut options.namespaces);
            let output = self.inner.validate_delta(&source, options)?;
            let report = Report::try_new(output.report)?.into_output();
            let pending = inplace.then(|| {
                Pending::inferred(
                    output.additions,
                    source.into_delta(),
                    !outcome.error_messages.is_empty(),
                    data.revision,
                )
            });
            Ok((report, pending))
        });
        Captured::complete(publisher, py, result).map(Some)
    }

    pub(super) fn infer_dataset(
        &self,
        py: Python<'_>,
        dataset: &NativeDataset,
        settings: &str,
        inplace: bool,
    ) -> PyResult<Option<Prepared<Output>>> {
        self.prepare_inferred_dataset(py, dataset, settings, inplace)
            .map(|prepared| {
                prepared.map(|(mut report, pending)| {
                    let warnings = std::mem::take(&mut report.3);
                    Prepared::with_pending(report, warnings, pending)
                })
            })
    }
}
