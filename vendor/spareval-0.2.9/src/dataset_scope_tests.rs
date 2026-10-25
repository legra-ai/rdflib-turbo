use super::*;
use oxrdf::{GraphName, NamedOrBlankNode, Quad};
use std::collections::HashSet;
fn externalize(row: InternalQuad<TermCow<'_>>) -> Quad {
    let subject = match Term::from(row.subject) {
        Term::NamedNode(n) => n.into(),
        Term::BlankNode(n) => NamedOrBlankNode::from(n),
        _ => panic!("invalid subject"),
    };
    let Term::NamedNode(predicate) = Term::from(row.predicate) else {
        panic!("invalid predicate")
    };
    let graph = match row.graph_name.map(Term::from) {
        None => GraphName::DefaultGraph,
        Some(Term::NamedNode(n)) => n.into(),
        Some(Term::BlankNode(n)) => n.into(),
        _ => panic!("invalid graph"),
    };
    Quad::new(subject, predicate, Term::from(row.object), graph)
}
fn fixture() -> Dataset {
    let objects: Vec<Term> = vec![
        Literal::new_simple_literal("value").into(),
        Literal::new_typed_literal(
            "value",
            NamedNode::new_unchecked("http://www.w3.org/2001/XMLSchema#string"),
        )
        .into(),
        Literal::new_typed_literal(
            "01",
            NamedNode::new_unchecked("http://www.w3.org/2001/XMLSchema#integer"),
        )
        .into(),
        BlankNode::new_from_unique_id(0).into(),
        BlankNode::new("0").unwrap().into(),
    ];
    #[cfg(feature = "sparql-12")]
    let objects: Vec<Term> = objects
        .into_iter()
        .chain([
            Triple::new(
                BlankNode::new_from_unique_id(u128::MAX),
                NamedNode::new_unchecked("urn:nested"),
                Literal::new_simple_literal("quoted"),
            )
            .into(),
            Literal::new_directional_language_tagged_literal("value", "en", BaseDirection::Ltr)
                .unwrap()
                .into(),
            Literal::new_directional_language_tagged_literal("value", "en", BaseDirection::Rtl)
                .unwrap()
                .into(),
        ])
        .collect();
    let mut data = Dataset::new();
    for object in objects {
        for graph in [
            GraphName::DefaultGraph,
            NamedNode::new_unchecked("urn:g").into(),
            BlankNode::new("graph").unwrap().into(),
            BlankNode::new_from_unique_id(0).into(),
            BlankNode::new("0").unwrap().into(),
        ] {
            data.insert(&Quad::new(
                NamedNode::new_unchecked("urn:s"),
                NamedNode::new_unchecked("urn:p"),
                object.clone(),
                graph,
            ));
        }
    }
    data
}
#[test]
fn graph_modes_and_all_term_masks_preserve_exact_results() {
    let data = fixture();
    let source = &data;
    for target in data.iter() {
        for mask in 0..8 {
            for mode in 0..7 {
                let s = (mask & 1 != 0)
                    .then(|| TermCow::Owned(Term::from(target.subject.into_owned())));
                let p = (mask & 2 != 0)
                    .then(|| TermCow::Owned(Term::from(target.predicate.into_owned())));
                let o = (mask & 4 != 0).then(|| TermCow::Owned(target.object.into_owned()));
                let graph = match mode {
                    0 | 4 => None,
                    1 => Some(TermCow::Owned(NamedNode::new_unchecked("urn:g").into())),
                    2 => Some(TermCow::Owned(BlankNode::new("graph").unwrap().into())),
                    5 => Some(TermCow::Owned(BlankNode::new_from_unique_id(0).into())),
                    6 => Some(TermCow::Owned(BlankNode::new("0").unwrap().into())),
                    _ => Some(TermCow::Owned(
                        NamedNode::new_unchecked("urn:missing").into(),
                    )),
                };
                let graph_filter = match mode {
                    0 => Some(None),
                    4 => None,
                    _ => Some(graph.as_ref()),
                };
                let exact_graph = graph.as_ref().map(|t| match TermRef::from(t) {
                    TermRef::NamedNode(n) => GraphNameRef::from(n),
                    TermRef::BlankNode(n) => GraphNameRef::from(n),
                    _ => unreachable!(),
                });
                let actual: HashSet<_> = source
                    .internal_quads_for_pattern(s.as_ref(), p.as_ref(), o.as_ref(), graph_filter)
                    .map(|r| externalize(r.unwrap()))
                    .collect();
                let wanted: HashSet<_> = data
                    .iter()
                    .filter(|q| {
                        (mask & 1 == 0 || q.subject == target.subject)
                            && (mask & 2 == 0 || q.predicate == target.predicate)
                            && (mask & 4 == 0 || q.object == target.object)
                            && match mode {
                                0 => q.graph_name.is_default_graph(),
                                4 => !q.graph_name.is_default_graph(),
                                _ => Some(q.graph_name) == exact_graph,
                            }
                    })
                    .map(QuadRef::into_owned)
                    .collect();
                assert_eq!(actual, wanted);
            }
        }
    }
    let actual: HashSet<_> = source
        .internal_quads_for_pattern(None, None, None, None)
        .map(|r| externalize(r.unwrap()))
        .collect();
    let expected: HashSet<_> = data
        .iter()
        .filter(|q| !q.graph_name.is_default_graph())
        .map(QuadRef::into_owned)
        .collect();
    assert_eq!(actual, expected);
}
#[test]
fn owned_call_patterns_can_drop_before_results_are_consumed() {
    let data = fixture();
    let source = &data;
    let rows = {
        let s = TermCow::Owned(NamedNode::new_unchecked("urn:s").into());
        let p = TermCow::Owned(NamedNode::new_unchecked("urn:p").into());
        let o = TermCow::Owned(Literal::new_simple_literal("value").into());
        let g = TermCow::Owned(NamedNode::new_unchecked("urn:g").into());
        source.internal_quads_for_pattern(Some(&s), Some(&p), Some(&o), Some(Some(&g)))
    };
    assert_eq!(rows.count(), 1);
}
#[test]
fn invalid_statement_positions_stay_empty() {
    let data = fixture();
    let source = &data;
    let literal = TermCow::Owned(Literal::new_simple_literal("value").into());
    let blank = TermCow::Owned(BlankNode::new("p").unwrap().into());
    assert_eq!(
        source
            .internal_quads_for_pattern(Some(&literal), None, None, Some(None))
            .count(),
        0
    );
    assert_eq!(
        source
            .internal_quads_for_pattern(None, Some(&blank), None, Some(None))
            .count(),
        0
    );
    assert_eq!(
        source
            .internal_quads_for_pattern(None, None, None, Some(Some(&literal)))
            .count(),
        0
    );
}
