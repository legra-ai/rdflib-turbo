//! A failure after real postprocessing removals must roll back every round.
use super::backend::Graph;
use super::{InferenceRequest, expand_configured};
use crate::{Error, Result};
use oxigraph::model::*;
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults};
use oxrdf::{GeneralizedDataset, GeneralizedTransaction};

struct AfterRemoval<'a, 'data> {
    transaction: &'a mut GeneralizedTransaction<'data>,
}
impl<'owner, 'data> Graph for AfterRemoval<'owner, 'data> {
    fn quads<'a>(
        &'a self,
        s: Option<TermRef<'_>>,
        p: Option<TermRef<'_>>,
        o: Option<TermRef<'_>>,
    ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a, 'owner, 'data> {
        self.transaction.quads(s, p, o)
    }
    fn query(&self, query: PreparedSparqlQuery) -> Result<QueryResults<'_>> {
        self.transaction.query(query)
    }
    fn inference_heads(
        &self,
        query: PreparedSparqlQuery,
    ) -> Result<oxigraph::sparql::QueryTripleIter<'_, GeneralizedTriple>> {
        self.transaction.inference_heads(query)
    }
    fn insert(&mut self, quad: GeneralizedQuad) -> Result<()> {
        Graph::insert(self.transaction, quad)
    }
    fn cleanup_blank_predicates(&mut self) -> Result<()> {
        assert!(
            self.transaction.inserted().next().is_some(),
            "real reasoning published tentative additions"
        );
        assert!(
            self.transaction
                .remove_where(|q| q.predicate.is_blank_node())
                > 0
        );
        Err(Error::Runtime("postprocess failed after removal".into()))
    }
}

fn source() -> GeneralizedDataset {
    GeneralizedDataset::from_iter([GeneralizedQuad::new(
        Literal::new_simple_literal("subject"),
        BlankNode::new_unchecked("predicate"),
        Literal::new_simple_literal("object"),
        GraphName::DefaultGraph,
    )])
}

#[test]
fn postprocess_error_after_real_removal_rolls_back_base_and_derived_facts() {
    let mut data = source();
    let original = data.clone(); // bounded: one synthetic row, exact rollback oracle.
    {
        let mut transaction = data.transaction();
        let mut graph = AfterRemoval {
            transaction: &mut transaction,
        };
        let result = expand_configured(&mut graph, "owlrl", InferenceRequest::default());
        assert!(
            matches!(result,Err(Error::Runtime(message)) if message=="postprocess failed after removal")
        );
    }
    assert_eq!(data, original);
}

#[test]
fn mutating_infer_delta_cleans_caller_but_returns_only_current_new_facts() {
    for destination in [
        None,
        Some(NamedNode::new_unchecked("urn:destination").into()),
    ] {
        let mut data = source();
        let original = data.clone(); // bounded: one synthetic row.
        let (delta, _) =
            super::expand_delta(&mut data, "owlrl", destination, InferenceRequest::default())
                .unwrap();
        assert!(data.iter().all(|q| !q.predicate.is_blank_node()));
        assert!(delta.iter().all(|q| !q.predicate.is_blank_node()));
        assert!(!delta.is_empty());
        for quad in delta.iter() {
            assert!(data.contains(quad));
            assert!(!original.contains(quad));
        }
        assert_eq!(data, delta, "the only original row was removed");
    }
}
