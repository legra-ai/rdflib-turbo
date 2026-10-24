//! Fallible derived-fact insertion must abort seeding, rounds and diagnostics.
use oxigraph::model::*;
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults};
use oxrdf::GeneralizedDataset;
use oxrdf::dataset::GeneralizedTransaction;

use super::backend::Graph;
use super::{InferenceOptions, diagnostics, expand_configured, literal_values};
use crate::{Error, Result};

const FAILURE: &str = "derived insertion lookup failed";

struct InsertionFault<'a, 'data> {
    journal: &'a mut GeneralizedTransaction<'data>,
    remaining: usize,
}

impl<'owner, 'data> Graph for InsertionFault<'owner, 'data> {
    fn quads<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
    ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a, 'owner, 'data> {
        self.journal
            .quads_for_pattern(subject, predicate, object, Some(GraphNameRef::DefaultGraph))
            .map(|quad| Ok(quad.into_owned()))
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
        if self.remaining == 0 {
            return Err(Error::Runtime(FAILURE.into()));
        }
        self.remaining -= 1;
        self.journal.insert(&quad);
        Ok(())
    }
}

fn triple(subject: &str, object: impl Into<Term>) -> GeneralizedQuad {
    GeneralizedQuad::new(
        NamedNode::new(format!("urn:{subject}")).unwrap(),
        NamedNode::new("urn:p").unwrap(),
        object,
        GraphName::DefaultGraph,
    )
}

fn error(result: Result<impl Sized>) {
    assert!(matches!(result, Err(Error::Runtime(text)) if text == FAILURE));
}

#[test]
fn configured_axiom_and_round_insertions_propagate_failures_and_rollback() {
    // bounded: one fixture and three fixed inference configurations.
    for (mode, options, remaining) in [
        (
            "rdfs",
            InferenceOptions {
                axiomatic_triples: true,
                datatype_axioms: false,
            },
            0,
        ),
        (
            "owlrl",
            InferenceOptions {
                axiomatic_triples: false,
                datatype_axioms: true,
            },
            0,
        ),
        ("rdfs", InferenceOptions::default(), 1),
    ] {
        let original = triple("source", NamedNode::new("urn:value").unwrap());
        let mut data = GeneralizedDataset::from_iter([&original]);
        {
            let mut journal = data.transaction();
            error(expand_configured(
                &mut InsertionFault {
                    journal: &mut journal,
                    remaining,
                },
                mode,
                options.into(),
            ));
        }
        assert_eq!(data.len(), 1);
        assert!(data.contains(&original));
        let mut recovery = data.transaction();
        recovery.insert(&triple("recovery", NamedNode::new("urn:value").unwrap()));
        recovery.commit();
        assert_eq!(data.len(), 2);
    }
}

#[test]
fn literal_substitution_and_diagnostic_insertions_propagate_failures() {
    let plain = triple("plain", Literal::new_simple_literal("word"));
    let typed = triple(
        "typed",
        Literal::new_typed_literal(
            "word",
            NamedNode::new("http://www.w3.org/2001/XMLSchema#normalizedString").unwrap(),
        ),
    );
    let mut data = GeneralizedDataset::from_iter([&plain, &typed]);
    {
        let mut journal = data.transaction();
        error(literal_values::substitute(
            &mut InsertionFault {
                journal: &mut journal,
                remaining: 0,
            },
            &Default::default(),
        ));
    }
    {
        let mut journal = data.transaction();
        error(
            diagnostics::Diagnostics::new(vec!["existing diagnostic".into()], 4300).finish(
                &mut InsertionFault {
                    journal: &mut journal,
                    remaining: 1,
                },
                false,
                |_| Ok(false),
            ),
        );
    }
    assert_eq!(data.len(), 2);
    assert!(data.contains(&plain) && data.contains(&typed));
}
