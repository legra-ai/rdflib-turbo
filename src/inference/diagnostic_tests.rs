//! Diagnostic failures retain the same real insertion-journal rollback boundary.

use oxigraph::model::*;
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults};
use oxrdf::GeneralizedDataset;
use oxrdf::dataset::GeneralizedTransaction;

use super::backend::Graph;
use super::{InferenceRequest, expand_inner};
use crate::{Error, Result};

struct Fault<'a, 'data> {
    journal: &'a mut GeneralizedTransaction<'data>,
    predicate: &'static str,
}

impl<'owner, 'data> Graph for Fault<'owner, 'data> {
    fn quads<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
    ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a, 'owner, 'data> {
        let failed = predicate.is_some_and(|predicate| matches!(predicate, TermRef::NamedNode(node) if node.as_str() == self.predicate));
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

    fn inference_heads(
        &self,
        query: PreparedSparqlQuery,
    ) -> Result<oxigraph::sparql::QueryTripleIter<'_, GeneralizedTriple>> {
        Ok(query
            .on_queryable_dataset(&**self.journal)
            .execute_inference_construct()?)
    }

    fn cleanup_blank_predicates(&mut self) -> Result<()> {
        self.journal.cleanup_blank_predicates()
    }

    fn insert(&mut self, quad: GeneralizedQuad) -> Result<()> {
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
        let original = GeneralizedQuad::new(
            NamedNode::new("urn:x").unwrap(),
            NamedNode::new("http://www.w3.org/1999/02/22-rdf-syntax-ns#type").unwrap(),
            NamedNode::new("http://www.w3.org/2002/07/owl#Nothing").unwrap(),
            GraphName::DefaultGraph,
        );
        let mut data = GeneralizedDataset::from_iter([original.clone()]);
        {
            let mut journal = data.transaction();
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
                        Some(
                            NamedNodeRef::new_unchecked(
                                "http://www.daml.org/2002/03/agents/agent-ont#error"
                            )
                            .into()
                        ),
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
