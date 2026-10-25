//! A late grouped query error never publishes pending facts or journal writes.
use oxrdf::GeneralizedDataset;
use std::cell::Cell;

use oxigraph::model::*;
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults};
use rdf_reasoning::RuleId;

use super::super::{Graph, Pending, apply_rules, staged::JournalGraph};
use super::CompiledRule;
use crate::{Error, Result};

struct LateQueryError<'a, 'data> {
    source: &'a JournalGraph<'data>,
    calls: Cell<usize>,
}

impl<'owner, 'data> Graph for LateQueryError<'owner, 'data> {
    fn quads<'a>(
        &'a self,
        s: Option<TermRef<'_>>,
        p: Option<TermRef<'_>>,
        o: Option<TermRef<'_>>,
    ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a, 'owner, 'data> {
        self.source.quads(s, p, o)
    }

    fn query(&self, query: PreparedSparqlQuery) -> Result<QueryResults<'_>> {
        let call = self.calls.get();
        self.calls.set(call + 1);
        if call == 1 {
            return Err(Error::Runtime("grouped late query failure".into()));
        }
        self.source.query(query)
    }

    fn inference_heads(
        &self,
        query: PreparedSparqlQuery,
    ) -> Result<oxigraph::sparql::QueryTripleIter<'_, GeneralizedTriple>> {
        let call = self.calls.get();
        self.calls.set(call + 1);
        if call == 1 {
            return Err(Error::Runtime("grouped late query failure".into()));
        }
        self.source.inference_heads(query)
    }

    fn cleanup_blank_predicates(&mut self) -> Result<()> {
        panic!("immutable kernel fixture must not postprocess storage")
    }

    fn insert(&mut self, _: GeneralizedQuad) -> Result<()> {
        panic!("immutable query must not publish")
    }
}

#[test]
fn late_grouped_query_failure_keeps_pending_private_and_rolls_back_prior_writes() {
    let n = |value| NamedNode::new(value).unwrap();
    let original = GeneralizedQuad::new(
        n("urn:C"),
        n("http://www.w3.org/1999/02/22-rdf-syntax-ns#type"),
        n("http://www.w3.org/2000/01/rdf-schema#Class"),
        GraphName::DefaultGraph,
    );
    let previous = GeneralizedQuad::new(
        n("urn:previous"),
        n("urn:p"),
        n("urn:o"),
        GraphName::DefaultGraph,
    );
    let mut data = GeneralizedDataset::from_iter([&original]);
    {
        let mut journal = JournalGraph::new(&mut data, GraphName::DefaultGraph);
        journal.insert(previous.clone()).unwrap();
        let source = LateQueryError {
            source: &journal,
            calls: Cell::new(0),
        };
        let queries = CompiledRule::try_catalogue(&[
            RuleId::Rdfs8.definition(),
            RuleId::Rdfs6.definition(),
            RuleId::Rdfs10.definition(),
        ])
        .unwrap();
        let mut pending = Pending::default();
        let error = apply_rules(&source, &queries, &mut pending, false).unwrap_err();
        assert!(
            matches!(error, Error::Runtime(message) if message == "grouped late query failure")
        );
        assert_eq!(source.calls.get(), 2);
        assert_eq!(
            pending.len(),
            2,
            "first grouped body must emit both heads before failure"
        );
        for quad in &pending {
            assert!(!journal.contains(quad).unwrap());
        }
        assert!(journal.contains((&previous).into()).unwrap());
        // The same production insertion journal drops without commit on error.
    }
    assert_eq!(data.len(), 1);
    assert!(data.contains(&original));
    assert!(!data.contains(&previous));
    let mut journal = JournalGraph::new(&mut data, GraphName::DefaultGraph);
    journal.insert(previous.clone()).unwrap();
    journal.commit();
    assert!(data.contains(&previous));
}
