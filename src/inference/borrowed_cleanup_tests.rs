//! Postprocessing is a full-Term logical removal overlay until acceptance.
use super::backend::Graph;
use super::borrowed::BorrowedInference;
use oxigraph::model::*;
use oxrdf::GeneralizedDataset;
use pyshacl_turbo_engine::GraphRead;

fn row(graph: GraphName) -> GeneralizedQuad {
    GeneralizedQuad::new(
        Literal::new_simple_literal("subject"),
        BlankNode::new_unchecked("predicate"),
        Literal::new_simple_literal("object"),
        graph,
    )
}

#[test]
fn borrowed_cleanup_preserves_source_until_acceptance_and_exact_graph_kinds() {
    let named = row(NamedNode::new_unchecked("urn:scope").into());
    let blank = row(BlankNode::new_unchecked("urn:scope").into());
    let default = row(GraphName::DefaultGraph);
    let unrelated = GeneralizedQuad::new(
        Literal::new_simple_literal("other"),
        NamedNode::new_unchecked("urn:p"),
        Literal::new_simple_literal("object"),
        GraphName::DefaultGraph,
    );
    let source = GeneralizedDataset::from_iter([
        named.clone(),
        blank.clone(),
        default.clone(),
        unrelated.clone(),
    ]);
    let mut view = BorrowedInference::new(&source, NamedNode::new_unchecked("urn:derived").into());
    view.insert(GeneralizedQuad::new(
        BlankNode::new_unchecked("new-subject"),
        BlankNode::new_unchecked("new-predicate"),
        Literal::new_simple_literal("new-object"),
        GraphName::DefaultGraph,
    ))
    .unwrap();
    view.cleanup_blank_predicates().unwrap();
    assert!(!Graph::contains(&view, named.as_ref()).unwrap());
    assert!(!view.contains_destination(named.as_ref()).unwrap());
    assert_eq!(view.quads(None, None, None).count(), 1);
    assert_eq!(view.matching_quads(None, None, None, None).count(), 1);
    assert!(source.contains(&named) && source.contains(&blank) && source.contains(&default));
    let delta = view.into_delta();
    assert_eq!(delta.removals().count(), 3);
    assert!(
        delta.additions.is_empty(),
        "transient heads are removed, not published"
    );
    let mut accepted = source;
    let mut transaction = accepted.transaction();
    delta.apply_dataset(&mut transaction);
    transaction.commit();
    assert_eq!(accepted, GeneralizedDataset::from_iter([unrelated]));
}

#[test]
fn borrowed_cleanup_never_filters_reader_errors_or_changes_empty_graph_declarations() {
    struct Broken(GeneralizedDataset);
    impl GraphRead for Broken {
        fn matching_quads<'a>(
            &'a self,
            s: Option<TermRef<'_>>,
            p: Option<TermRef<'_>>,
            o: Option<TermRef<'_>>,
            g: Option<GraphNameRef<'_>>,
        ) -> impl Iterator<Item = pyshacl_turbo_engine::Result<GeneralizedQuad>> + use<'a> {
            self.0
                .quads_for_pattern(s, p, o, g)
                .map(|q| Ok(q.into_owned()))
                .chain(std::iter::once(Err(pyshacl_turbo_engine::Error::Runtime(
                    "cleanup read failed".into(),
                ))))
        }
        fn contains_named_graph(
            &self,
            graph: &NamedOrBlankNode,
        ) -> pyshacl_turbo_engine::Result<bool> {
            Ok(graph == &NamedOrBlankNode::from(NamedNode::new_unchecked("urn:empty")))
        }
        fn named_graphs(
            &self,
        ) -> impl Iterator<Item = pyshacl_turbo_engine::Result<NamedOrBlankNode>> + '_ {
            std::iter::once(Ok(NamedNode::new_unchecked("urn:empty").into()))
        }
    }
    let source = Broken(GeneralizedDataset::from_iter([row(
        GraphName::DefaultGraph,
    )]));
    let mut view = BorrowedInference::new(&source, GraphName::DefaultGraph);
    assert!(view.cleanup_blank_predicates().is_err());
    assert_eq!(source.0.len(), 1);
    assert!(
        view.contains_named_graph(&NamedNode::new_unchecked("urn:empty").into())
            .unwrap()
    );
}
