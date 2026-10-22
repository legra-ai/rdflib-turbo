//! Owned metadata from one successful, possibly provisional inference invocation.

use pyo3::prelude::*;

pub(crate) const ERROR_NAMESPACE: &str = "http://www.daml.org/2002/03/agents/agent-ont#";

#[pyclass(frozen, name = "NativeInferenceOutcome")]
#[derive(Debug, Default)]
pub(crate) struct InferenceOutcome {
    // Output-sized: one message per diagnostic node emitted by this invocation.
    #[pyo3(get)]
    pub(crate) error_messages: Vec<String>,
}

impl InferenceOutcome {
    pub(crate) fn bind_namespace(&self, namespaces: &mut Vec<(String, String)>) {
        if self.error_messages.is_empty() {
            return;
        }
        bind_error_namespace(namespaces);
    }
}

pub(crate) fn bind_error_namespace(namespaces: &mut Vec<(String, String)>) {
    let mut prefix = "err".to_owned();
    let mut suffix = 0;
    loop {
        match namespaces.iter().find(|(bound, _)| bound == &prefix) {
            Some((_, namespace)) if namespace == ERROR_NAMESPACE => return,
            Some(_) => {
                suffix += 1;
                prefix = format!("err{suffix}");
            }
            None => {
                namespaces.retain(|(_, namespace)| namespace != ERROR_NAMESPACE);
                namespaces.push((prefix, ERROR_NAMESPACE.to_owned()));
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_binding_matches_rdflib_prefix_collision_and_rebinding() {
        for (mut namespaces, expected) in [
            (vec![], vec![("err".into(), ERROR_NAMESPACE.into())]),
            (
                vec![("err".into(), "urn:other:".into())],
                vec![
                    ("err".into(), "urn:other:".into()),
                    ("err1".into(), ERROR_NAMESPACE.into()),
                ],
            ),
            (
                vec![
                    ("err".into(), "urn:other:".into()),
                    ("err1".into(), "urn:more:".into()),
                ],
                vec![
                    ("err".into(), "urn:other:".into()),
                    ("err1".into(), "urn:more:".into()),
                    ("err2".into(), ERROR_NAMESPACE.into()),
                ],
            ),
            (
                vec![("custom".into(), ERROR_NAMESPACE.into())],
                vec![("err".into(), ERROR_NAMESPACE.into())],
            ),
        ] {
            bind_error_namespace(&mut namespaces);
            assert_eq!(namespaces, expected);
        }
    }
}
