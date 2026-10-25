//! Optional native-union streams are authoritative sources, never retry hints.
use std::cell::Cell;
use std::io;
use std::rc::Rc;

use oxrdf::{NamedNode, Term};
use spargebra::SparqlParser;

use crate::{
    CancellationToken, InternalQuad, QueryEvaluationError, QueryEvaluator, QueryResults,
    QueryableDataset,
};

struct Dataset {
    supplied: bool,
    raw: Rc<Cell<usize>>,
    native: Rc<Cell<usize>>,
}

impl<'a> QueryableDataset<'a> for Dataset {
    type InternalTerm = Term;
    type Error = io::Error;

    fn internal_quads_for_pattern(
        &self,
        subject: Option<&Term>,
        predicate: Option<&Term>,
        object: Option<&Term>,
        graph: Option<Option<&Term>>,
    ) -> impl Iterator<Item = Result<InternalQuad<Term>, io::Error>> + use<'a> {
        self.raw.set(self.raw.get() + 1);
        let (subject, predicate, object) = (subject.cloned(), predicate.cloned(), object.cloned());
        let graph = graph.map(|g| g.cloned());
        let row: InternalQuad<Term> = InternalQuad {
            subject: NamedNode::new_unchecked("urn:s").into(),
            predicate: NamedNode::new_unchecked("urn:p").into(),
            object: NamedNode::new_unchecked("urn:o").into(),
            graph_name: None,
        };
        let named = InternalQuad {
            subject: row.subject.clone(),
            predicate: row.predicate.clone(),
            object: row.object.clone(),
            graph_name: Some(NamedNode::new_unchecked("urn:g").into()),
        };
        [row, named]
            .into_iter()
            .filter(move |row| {
                subject.as_ref().is_none_or(|s| s == &row.subject)
                    && predicate.as_ref().is_none_or(|p| p == &row.predicate)
                    && object.as_ref().is_none_or(|o| o == &row.object)
                    && match &graph {
                        None => true,
                        Some(None) => row.graph_name.is_none(),
                        Some(Some(g)) => row.graph_name.as_ref() == Some(g),
                    }
            })
            .map(Ok)
    }

    fn internal_unique_default_union_quads_for_pattern(
        &self,
        _: Option<&Term>,
        _: Option<&Term>,
        _: Option<&Term>,
        token: &CancellationToken,
    ) -> Option<Box<dyn Iterator<Item = Result<InternalQuad<Term>, QueryEvaluationError>> + 'a>>
    {
        self.native.set(self.native.get() + 1);
        if !self.supplied {
            return None;
        }
        let error = if token.is_cancelled() {
            QueryEvaluationError::Cancelled
        } else {
            QueryEvaluationError::Dataset(Box::new(io::Error::other(
                "supplied union source failed",
            )))
        };
        Some(Box::new(std::iter::once(Err(error))))
    }

    fn internalize_term(&self, term: Term) -> Result<Term, io::Error> {
        Ok(term)
    }
    fn externalize_term(&self, term: Term) -> Result<Term, io::Error> {
        Ok(term)
    }
}

fn run(supplied: bool, selection: usize) -> (Result<usize, QueryEvaluationError>, usize, usize) {
    let raw = Rc::new(Cell::new(0));
    let native = Rc::new(Cell::new(0));
    let dataset = Dataset {
        supplied,
        raw: Rc::clone(&raw),
        native: Rc::clone(&native),
    };
    let query = SparqlParser::new()
        .parse_query("SELECT ?s WHERE { ?s ?p ?o }")
        .unwrap();
    let evaluator = QueryEvaluator::new();
    let mut prepared = evaluator.prepare(&query);
    match selection {
        0 => {}
        1 => prepared.dataset_mut().set_default_graph_as_union(),
        2 => prepared
            .dataset_mut()
            .set_default_graph(vec![NamedNode::new_unchecked("urn:g").into()]),
        _ => panic!("invalid graph selection"),
    }
    let result = match prepared.execute(dataset) {
        Ok(QueryResults::Solutions(mut rows)) => {
            rows.try_fold(0, |count, row| row.map(|_| count + 1))
        }
        Ok(_) => panic!("SELECT must yield solutions"),
        Err(error) => Err(error),
    };
    (result, raw.get(), native.get())
}

#[test]
#[cfg(feature = "rdf-set-union")]
fn supplied_union_error_propagates_without_reading_generic_rows() {
    let (result, raw, native) = run(true, 1);
    let error = result.expect_err("the supplied stream must be authoritative");
    assert_eq!(error.to_string(), "supplied union source failed");
    assert_eq!((raw, native), (0, 1));
}

#[test]
fn absent_union_capability_and_disabled_feature_preserve_existing_generic_semantics() {
    let (result, raw, native) = run(false, 1);
    assert_eq!(
        result.unwrap(),
        if cfg!(feature = "rdf-set-union") {
            1
        } else {
            2
        }
    );
    assert_eq!(raw, 1);
    assert_eq!(native, usize::from(cfg!(feature = "rdf-set-union")));
}

#[test]
fn single_default_and_selected_graphs_do_not_invoke_union_capability() {
    for selection in [0, 2] {
        let (result, raw, native) = run(true, selection);
        assert_eq!(result.unwrap(), 1);
        assert_eq!((raw, native), (1, 0));
    }
}
