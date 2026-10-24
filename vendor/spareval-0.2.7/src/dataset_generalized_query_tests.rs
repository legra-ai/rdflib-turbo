use super::*;
use crate::{QueryEvaluator, QueryResults};
#[cfg(feature = "sparql-12")]
use oxrdf::GraphName;
use oxrdf::{GeneralizedDataset, GeneralizedQuad};
use std::cell::RefCell;
use std::collections::HashSet;
use std::io;
use std::rc::Rc;

struct ScopedSource<'a> {
    data: &'a GeneralizedDataset,
    seen: Rc<RefCell<Vec<Vec<Term>>>>,
    fail: bool,
    reject_nodes: bool,
}

impl<'a> QueryableDataset<'a> for ScopedSource<'a> {
    type InternalTerm = TermCow<'a>;
    type Error = io::Error;

    fn scoped_custom_function(&self, name: &NamedNode) -> Option<ScopedCustomFunction<'a>> {
        if name.as_str() != "urn:capture" {
            return None;
        }
        let seen = self.seen.clone();
        Some(Rc::new(move |args| {
            seen.borrow_mut().push(args.to_vec());
            Some(Literal::from(true).into())
        }))
    }

    fn internal_quads_for_pattern(
        &self,
        s: Option<&TermCow<'a>>,
        p: Option<&TermCow<'a>>,
        o: Option<&TermCow<'a>>,
        g: Option<Option<&TermCow<'a>>>,
    ) -> impl Iterator<Item = Result<InternalQuad<TermCow<'a>>, io::Error>> + use<'a> {
        self.fail
            .then(|| Err(io::Error::other("generalized source read failed")))
            .into_iter()
            .chain(
                self.data
                    .internal_quads_for_pattern(s, p, o, g)
                    .map(|r| r.map_err(|never| match never {})),
            )
    }

    fn internal_named_graphs(
        &self,
    ) -> impl Iterator<Item = Result<TermCow<'a>, io::Error>> + use<'a> {
        self.data
            .internal_named_graphs()
            .map(|r| r.map_err(|never| match never {}))
    }

    fn internalize_term(&self, term: Term) -> Result<TermCow<'a>, io::Error> {
        if self.reject_nodes && matches!(term, Term::NamedNode(_) | Term::BlankNode(_)) {
            return Err(io::Error::other("new nodes must not be internalized"));
        }
        Ok(term.into())
    }

    fn externalize_term(&self, term: TermCow<'a>) -> Result<Term, io::Error> {
        Ok(term.into())
    }
}

#[test]
fn generalized_scoped_callbacks_keep_common_rare_and_graph_term_roles() {
    let mut data = GeneralizedDataset::new();
    for row in [
        GeneralizedQuad::new(
            NamedNode::new_unchecked("urn:s"),
            BlankNode::new("predicate").unwrap(),
            Literal::new_simple_literal("common"),
            NamedNode::new_unchecked("urn:graph"),
        ),
        GeneralizedQuad::new(
            Literal::new_typed_literal(
                "01",
                NamedNode::new_unchecked("http://www.w3.org/2001/XMLSchema#integer"),
            ),
            BlankNode::new_from_unique_id(u128::MAX),
            Literal::new_typed_literal("lexical", NamedNode::new_unchecked("urn:datatype")),
            BlankNode::new_from_unique_id(0),
        ),
    ] {
        data.insert(row.as_ref());
    }
    let before: HashSet<_> = data.iter().map(|r| r.into_owned()).collect();
    let expected: HashSet<Vec<Term>> = data
        .iter()
        .map(|row| {
            let graph = match row.graph_name {
                GraphNameRef::NamedNode(n) => Term::from(n.into_owned()),
                GraphNameRef::BlankNode(n) => Term::from(n.into_owned()),
                GraphNameRef::DefaultGraph => panic!("expected named context"),
            };
            [
                row.subject.into_owned(),
                row.predicate.into_owned(),
                row.object.into_owned(),
                graph,
            ]
            .into_iter()
            .map(|term| {
                if cfg!(feature = "exact-custom-function-terms") {
                    term
                } else {
                    ExpressionTerm::from(term).into()
                }
            })
            .collect()
        })
        .collect();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let source = ScopedSource {
        data: &data,
        seen: seen.clone(),
        fail: false,
        reject_nodes: false,
    };
    let query = spargebra::SparqlParser::new()
        .parse_query(
            "SELECT (<urn:capture>(?s, ?p, ?o, ?g) AS ?captured) WHERE { GRAPH ?g { ?s ?p ?o } }",
        )
        .unwrap();
    let evaluator = QueryEvaluator::new();
    let QueryResults::Solutions(rows) = evaluator.prepare(&query).execute(source).unwrap() else {
        panic!("expected SELECT")
    };
    let rows = rows.map(Result::unwrap).collect::<Vec<_>>();
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .all(|r| r.get("captured") == Some(&Literal::from(true).into()))
    );
    assert_eq!(
        seen.borrow().iter().cloned().collect::<HashSet<_>>(),
        expected
    );
    assert_eq!(
        data.iter().map(|r| r.into_owned()).collect::<HashSet<_>>(),
        before
    );
}

#[test]
fn generalized_source_errors_are_not_empty_results() {
    let data = GeneralizedDataset::new();
    let query = spargebra::SparqlParser::new()
        .parse_query("SELECT ?s WHERE { ?s ?p ?o }")
        .unwrap();
    let source = ScopedSource {
        data: &data,
        seen: Rc::default(),
        fail: true,
        reject_nodes: false,
    };
    let evaluator = QueryEvaluator::new();
    match evaluator.prepare(&query).execute(source) {
        Err(error) => assert!(error.to_string().contains("generalized source read failed")),
        Ok(QueryResults::Solutions(mut rows)) => {
            let error = rows
                .next()
                .expect("source failure must be visible")
                .unwrap_err();
            assert!(error.to_string().contains("generalized source read failed"));
        }
        _ => panic!("expected SELECT error"),
    }
}

#[test]
fn generalized_construct_constants_and_generated_blanks_do_not_internalize_new_nodes() {
    let data = GeneralizedDataset::new();
    let source = ScopedSource {
        data: &data,
        seen: Rc::default(),
        fail: false,
        reject_nodes: true,
    };
    let query = spargebra::SparqlParser::new().parse_query(
        "CONSTRUCT { <urn:new> <urn:p> ?value . _:fresh <urn:p> ?value } WHERE { VALUES ?value { 1 1 } }"
    ).unwrap();
    let evaluator = QueryEvaluator::new();
    let QueryResults::Graph(rows) = evaluator.prepare(&query).execute(source).unwrap() else {
        panic!("expected CONSTRUCT")
    };
    let rows = rows.map(Result::unwrap).collect::<Vec<_>>();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows.iter().filter(|r| r.subject.is_blank_node()).count(), 2);
    assert!(data.is_empty());
}

#[cfg(feature = "sparql-12")]
#[test]
fn generalized_quoted_roles_and_directional_literals_remain_borrowed() {
    let literal =
        Literal::new_directional_language_tagged_literal("value", "en", BaseDirection::Rtl)
            .unwrap();
    let quoted = Triple::new(
        BlankNode::new_from_unique_id(u128::MAX),
        NamedNode::new_unchecked("urn:p"),
        literal.clone(),
    );
    let mut data = GeneralizedDataset::new();
    data.insert(oxrdf::GeneralizedQuadRef {
        subject: (&quoted).into(),
        predicate: (&quoted).into(),
        object: literal.as_ref().into(),
        graph_name: GraphName::DefaultGraph.as_ref(),
    });
    let source = &data;
    let row = source
        .internal_quads_for_pattern(None, None, None, Some(None))
        .next()
        .unwrap()
        .unwrap();
    assert!(matches!(row.subject, TermCow::Borrowed(TermRef::Triple(_))));
    assert!(matches!(
        row.predicate,
        TermCow::Borrowed(TermRef::Triple(_))
    ));
    assert!(matches!(row.object, TermCow::Borrowed(TermRef::Literal(_))));
    assert_eq!(TermRef::from(&row.object), literal.as_ref().into());
}
