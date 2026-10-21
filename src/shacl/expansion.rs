//! Rules use native dataset ownership without constructing a validation report.

use std::sync::RwLock;

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

use super::commit::Prepared;
use super::{Settings, SharedShapes, error};
use crate::{DatasetState, NativeDataset};

pub(super) type Output = (Option<NativeDataset>, Vec<String>);

impl SharedShapes {
    pub(super) fn expand_dataset(
        &self,
        py: Python<'_>,
        dataset: &NativeDataset,
        settings: &str,
        inplace: bool,
    ) -> PyResult<Output> {
        let mut options = Settings::parse(settings)?;
        let prepared = py.detach(|| -> PyResult<Prepared<Output>> {
            if inplace {
                let mut current = dataset
                    .inner
                    .write()
                    .map_err(|_| PyRuntimeError::new_err("Dataset lock poisoned"))?;
                let revision = current.next_revision()?;
                let expected_revision = current.revision;
                options.namespaces = current.namespaces.clone();
                let expansion = self
                    .inner
                    .expand_staged(&current.data, options)
                    .map_err(error)?;
                let warnings = expansion.recursion_warnings;
                if warnings.is_empty() {
                    current.extend(expansion.graph.iter());
                    current.revision = revision;
                    Ok(Prepared::ready((None, Vec::new()), warnings))
                } else {
                    Ok(Prepared::staged(
                        (None, Vec::new()),
                        expansion.graph,
                        expected_revision,
                        warnings,
                    ))
                }
            } else {
                let mut working = dataset
                    .inner
                    .read()
                    .map_err(|_| PyRuntimeError::new_err("Dataset lock poisoned"))?
                    .copy_owner();
                options.namespaces = working.namespaces.clone();
                let warnings = self
                    .inner
                    .expand_rules(&mut working, options)
                    .map_err(error)?;
                let result = NativeDataset {
                    inner: RwLock::new(DatasetState::new(working.data, working.namespaces)),
                };
                Ok(Prepared::ready((Some(result), Vec::new()), warnings))
            }
        })?;
        prepared.finish(py, dataset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULE: &str = "@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:S> sh:targetNode <urn:a>; sh:rule [a sh:TripleRule; sh:subject sh:this; sh:predicate <urn:derived>; sh:object <urn:b>].";

    #[test]
    fn expansion_returns_an_independent_result_or_commits_to_the_original() {
        Python::initialize();
        Python::attach(|py| {
            let data = NativeDataset::new();
            data.load(
                py,
                "@prefix ex:<urn:>. ex:a ex:p ex:b.",
                "turtle",
                None,
                true,
            )
            .unwrap();
            let before = data.dump(py, "nquads").unwrap();
            let shapes = SharedShapes::new(py, RULE, true, vec![], true).unwrap();
            let (result, warnings) = shapes.expand(py, &data, "{}", false).unwrap();
            let result = result.unwrap();
            assert!(warnings.is_empty());
            assert_eq!(data.dump(py, "nquads").unwrap(), before);
            assert_eq!(result.__len__().unwrap(), 2);
            assert_eq!(result.namespaces().unwrap(), data.namespaces().unwrap());
            assert_eq!(result.inner.read().unwrap().revision, 0);
            let (result, warnings) = shapes.expand(py, &data, "{}", true).unwrap();
            assert!(result.is_none() && warnings.is_empty());
            assert_eq!(data.__len__().unwrap(), 2);
            assert_eq!(data.inner.read().unwrap().revision, 2);
            shapes
                .expand(py, &data, r#"{"use_shapes":["urn:other"]}"#, true)
                .unwrap();
            assert_eq!(data.inner.read().unwrap().revision, 3);
        });
    }

    #[test]
    fn late_rule_failure_keeps_original_graph_and_revision() {
        Python::initialize();
        Python::attach(|py| {
            let data = NativeDataset::new();
            data.load(py, "<urn:a> <urn:p> <urn:b>.", "turtle", None, true)
                .unwrap();
            let before = data.dump(py, "nquads").unwrap();
            let shapes = SharedShapes::new(py, RULE, true, vec![], true).unwrap();
            let failure = shapes
                .expand(
                    py,
                    &data,
                    r#"{"iterate_rules":true,"max_rule_iterations":1}"#,
                    true,
                )
                .err()
                .unwrap();
            assert!(failure.to_string().contains("iteration limit"));
            assert_eq!(data.dump(py, "nquads").unwrap(), before);
            assert_eq!(data.inner.read().unwrap().revision, 1);
        });
    }
}
