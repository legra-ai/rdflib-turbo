//! Rules use native dataset ownership without constructing a validation report.

use std::sync::RwLock;

use pyo3::prelude::*;

use super::commit::Prepared;
use super::settings::Captured;
use super::{Settings, SharedShapes};
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
        let (mut options, publisher) =
            Settings::capture_rules(py, settings, self.comparison_host.as_ref())?;
        let result = py.detach(|| -> crate::Result<Prepared<Output>> {
            if inplace {
                let current = dataset
                    .inner
                    .write()
                    .map_err(|_| crate::Error::Runtime("Dataset lock poisoned".into()))?;
                current.next_revision()?;
                let expected_revision = current.revision;
                options.namespaces = current.namespaces.clone();
                let expansion = self.inner.expand_staged(&current.data, options)?;
                let warnings = expansion.recursion_warnings;
                Ok(Prepared::staged(
                    (None, Vec::new()),
                    expansion.graph,
                    expected_revision,
                    warnings,
                ))
            } else {
                let mut working = dataset
                    .inner
                    .read()
                    .map_err(|_| crate::Error::Runtime("Dataset lock poisoned".into()))?
                    .copy_owner();
                options.namespaces = working.namespaces.clone();
                let expansion = self.inner.expand_staged(&working.data, options)?;
                working.extend(expansion.graph.iter());
                let warnings = expansion.recursion_warnings;
                let result = NativeDataset {
                    inner: RwLock::new(DatasetState::new(working.data, working.namespaces)),
                };
                Ok(Prepared::ready((Some(result), Vec::new()), warnings))
            }
        });
        let prepared = Captured::complete(publisher, py, result)?;
        prepared.finish(py, dataset, 2)
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
            let shapes = SharedShapes::new(
                py,
                RULE,
                true,
                vec![],
                true,
                crate::native_tests::host_regex_policy(py),
                crate::native_tests::host_comparison_policy(py),
            )
            .unwrap();
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
            let shapes = SharedShapes::new(
                py,
                RULE,
                true,
                vec![],
                true,
                crate::native_tests::host_regex_policy(py),
                crate::native_tests::host_comparison_policy(py),
            )
            .unwrap();
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
