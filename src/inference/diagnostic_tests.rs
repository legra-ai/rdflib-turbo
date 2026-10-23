//! Diagnostic failures retain the same real insertion-journal rollback boundary.

use oxigraph::model::*;
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults};
use oxrdf::dataset::InsertTransaction;

use super::backend::Graph;
use super::{InferenceRequest, expand_inner};
use crate::{Error, Result};

struct Fault<'a, 'data> {
    journal: &'a mut InsertTransaction<'data>,
    predicate: &'static str,
}

impl Graph for Fault<'_, '_> {
    fn quads<'a>(
        &'a self,
        subject: Option<NamedOrBlankNodeRef<'a>>,
        predicate: Option<NamedNodeRef<'a>>,
        object: Option<TermRef<'a>>,
    ) -> impl Iterator<Item = Result<Quad>> + 'a {
        let failed = predicate.is_some_and(|predicate| predicate.as_str() == self.predicate);
        let rows = self.journal.quads_for_pattern(
            subject,
            predicate,
            object,
            Some(GraphNameRef::DefaultGraph),
        );
        // The failure is delivered by the iterator, including an otherwise empty lookup.
        failed
            .then(|| Err(Error::Runtime("diagnostic storage read failed".into())))
            .into_iter()
            .chain(rows.map(|quad| Ok(quad.into_owned())))
    }

    fn query(&self, query: PreparedSparqlQuery) -> Result<QueryResults<'_>> {
        Ok(query.on_queryable_dataset(&**self.journal).execute()?)
    }

    fn insert(&mut self, quad: Quad) -> Result<()> {
        self.journal.insert(&quad);
        Ok(())
    }
}

#[test]
fn diagnostic_iterator_failure_rolls_back_derived_facts_and_publishes_no_error_nodes() {
    for predicate in [
        "http://www.w3.org/2002/07/owl#differentFrom",
        "http://www.w3.org/2002/07/owl#sourceIndividual",
    ] {
        let original = Quad::new(
            NamedNode::new("urn:x").unwrap(),
            NamedNode::new("http://www.w3.org/1999/02/22-rdf-syntax-ns#type").unwrap(),
            NamedNode::new("http://www.w3.org/2002/07/owl#Nothing").unwrap(),
            GraphName::DefaultGraph,
        );
        let mut data = Dataset::from_iter([original.clone()]);
        {
            let mut journal = data.insert_transaction();
            let failure = expand_inner(
                &mut Fault {
                    journal: &mut journal,
                    predicate,
                },
                "owlrl",
                InferenceRequest::default(),
            )
            .unwrap_err();
            assert_eq!(failure.to_string(), "diagnostic storage read failed");
            assert!(
                journal.len() > 1,
                "failure must follow actual staged inference"
            );
            assert_eq!(
                journal
                    .quads_for_pattern(
                        None,
                        Some(NamedNodeRef::new_unchecked(
                            "http://www.daml.org/2002/03/agents/agent-ont#error"
                        )),
                        None,
                        None
                    )
                    .count(),
                0
            );
        }
        assert_eq!(data.len(), 1);
        assert!(data.contains(&original));
        let outcome = super::expand(&mut data, "owlrl", InferenceRequest::default()).unwrap();
        assert_eq!(
            outcome.error_messages,
            ["urn:x is defined of type 'Nothing'"]
        );
    }
}
