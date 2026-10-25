use super::*;
use oxrdf::{GeneralizedDataset, GeneralizedQuad, GeneralizedQuadRef, GraphName};
use std::collections::HashSet;

fn fixture() -> GeneralizedDataset {
    let roles: [Term; 5] = [
        NamedNode::new_unchecked("urn:same").into(),
        BlankNode::new_unchecked("urn:same").into(),
        Literal::new_simple_literal("same").into(),
        BlankNode::new_from_unique_id(0).into(),
        BlankNode::new("0").unwrap().into(),
    ];
    let graphs: [GraphName; 5] = [
        GraphName::DefaultGraph,
        NamedNode::new_unchecked("urn:same").into(),
        BlankNode::new_unchecked("urn:same").into(),
        BlankNode::new_from_unique_id(0).into(),
        BlankNode::new("0").unwrap().into(),
    ];
    let mut data = GeneralizedDataset::new();
    for subject in &roles {
        for predicate in &roles {
            for graph in &graphs {
                data.insert(GeneralizedQuadRef {
                    subject: subject.as_ref(),
                    predicate: predicate.as_ref(),
                    object: Literal::new_simple_literal("object").as_ref().into(),
                    graph_name: graph.as_ref(),
                });
            }
        }
    }
    data
}

fn externalize(row: InternalQuad<TermCow<'_>>) -> GeneralizedQuad {
    GeneralizedQuad::new(
        Term::from(row.subject),
        Term::from(row.predicate),
        Term::from(row.object),
        match row.graph_name.map(Term::from) {
            None => GraphName::DefaultGraph,
            Some(Term::NamedNode(node)) => node.into(),
            Some(Term::BlankNode(node)) => node.into(),
            _ => panic!("invalid graph kind"),
        },
    )
}

#[test]
fn generalized_adapter_retains_all_roles_masks_and_exact_graph_kinds() {
    let data = fixture();
    let source = &data;
    for target in data.iter() {
        for mask in 0..8 {
            let s = (mask & 1 != 0).then(|| TermCow::Owned(target.subject.into_owned()));
            let p = (mask & 2 != 0).then(|| TermCow::Owned(target.predicate.into_owned()));
            let o = (mask & 4 != 0).then(|| TermCow::Owned(target.object.into_owned()));
            for mode in 0..3 {
                let g = match target.graph_name {
                    GraphNameRef::NamedNode(n) => Some(TermCow::Borrowed(n.into())),
                    GraphNameRef::BlankNode(n) => Some(TermCow::Borrowed(n.into())),
                    GraphNameRef::DefaultGraph => None,
                };
                let filter = match mode {
                    0 => Some(None),
                    1 => None,
                    _ => Some(g.as_ref()),
                };
                let wanted: HashSet<_> = data
                    .iter()
                    .filter(|q| {
                        (mask & 1 == 0 || q.subject == target.subject)
                            && (mask & 2 == 0 || q.predicate == target.predicate)
                            && (mask & 4 == 0 || q.object == target.object)
                            && match mode {
                                0 => q.graph_name.is_default_graph(),
                                1 => !q.graph_name.is_default_graph(),
                                _ => q.graph_name == target.graph_name,
                            }
                    })
                    .map(GeneralizedQuadRef::into_owned)
                    .collect();
                let actual: HashSet<_> = source
                    .internal_quads_for_pattern(s.as_ref(), p.as_ref(), o.as_ref(), filter)
                    .map(|row| externalize(row.unwrap()))
                    .collect();
                assert_eq!(actual, wanted, "mask={mask}, mode={mode}");
            }
        }
    }
}

#[test]
fn generalized_adapter_borrows_long_terms_after_temporary_patterns_drop() {
    let text = "wide🌍".repeat(32_768);
    let value = Literal::new_simple_literal(text);
    let graph = NamedNode::new_unchecked("urn:long");
    let mut data = GeneralizedDataset::new();
    data.insert(GeneralizedQuadRef {
        subject: value.as_ref().into(),
        predicate: value.as_ref().into(),
        object: value.as_ref().into(),
        graph_name: graph.as_ref().into(),
    });
    let direct = data.iter().next().unwrap();
    let TermRef::Literal(stored) = direct.subject else {
        panic!("expected literal")
    };
    let source = &data;
    let mut rows = {
        let s = TermCow::Owned(value.clone().into());
        let p = TermCow::Owned(value.clone().into());
        let g = TermCow::Owned(graph.into());
        source.internal_quads_for_pattern(Some(&s), Some(&p), None, Some(Some(&g)))
    };
    let row = rows.next().unwrap().unwrap();
    for term in [row.subject, row.predicate, row.object] {
        let TermCow::Borrowed(TermRef::Literal(value)) = term else {
            panic!("matching rows must borrow their dataset text")
        };
        assert_eq!(value.value().as_ptr(), stored.value().as_ptr());
    }
    assert!(matches!(row.graph_name, Some(TermCow::Borrowed(_))));
    assert!(rows.next().is_none());
}

#[test]
fn generalized_adapter_graph_names_borrow_and_reject_non_graph_terms() {
    let data = fixture();
    let source = &data;
    let actual: HashSet<_> = source
        .internal_named_graphs()
        .map(|row| match row.unwrap() {
            TermCow::Borrowed(term) => term.into_owned(),
            _ => panic!("graph identities must borrow their owner"),
        })
        .collect();
    assert_eq!(actual.len(), 4);
    let literal = TermCow::Owned(Literal::new_simple_literal("urn:same").into());
    assert_eq!(
        source
            .internal_quads_for_pattern(None, None, None, Some(Some(&literal)))
            .count(),
        0
    );
    assert!(!source.contains_internal_graph_name(&literal).unwrap());
    for name in actual {
        assert!(source.contains_internal_graph_name(&name.into()).unwrap());
    }
}

#[test]
fn generalized_select_ask_join_full_terms_while_construct_stays_strict() {
    use crate::{QueryEvaluator, QueryResults};
    let mut data = GeneralizedDataset::new();
    let subject = Literal::new_simple_literal("subject");
    let predicate = Literal::new_simple_literal("predicate");
    let bridge = Literal::new_simple_literal("bridge");
    let next = NamedNode::new_unchecked("urn:next");
    let end = NamedNode::new_unchecked("urn:end");
    for row in [
        GeneralizedQuad::new(
            subject.clone(),
            predicate,
            bridge.clone(),
            GraphName::DefaultGraph,
        ),
        GeneralizedQuad::new(bridge, next.clone(), end.clone(), GraphName::DefaultGraph),
        GeneralizedQuad::new(end.clone(), next, end, GraphName::DefaultGraph),
    ] {
        data.insert(row.as_ref());
    }
    let evaluator = QueryEvaluator::new();
    let ask = spargebra::SparqlParser::new()
        .parse_query("ASK { VALUES (?s ?p) { (\"subject\" \"predicate\") } ?s ?p ?o }")
        .unwrap();
    assert!(matches!(
        evaluator.prepare(&ask).execute(&data).unwrap(),
        QueryResults::Boolean(true)
    ));
    let select = spargebra::SparqlParser::new().parse_query(
        "SELECT ?s ?p ?tail WHERE { ?s ?p ?middle . ?middle <urn:next> ?tail FILTER(isLiteral(?s)) }"
    ).unwrap();
    let QueryResults::Solutions(rows) = evaluator.prepare(&select).execute(&data).unwrap() else {
        panic!("expected SELECT")
    };
    let rows = rows.map(Result::unwrap).collect::<Vec<_>>();
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .any(|row| row.get("s") == Some(&subject.clone().into()))
    );
    let construct = spargebra::SparqlParser::new()
        .parse_query("CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }")
        .unwrap();
    let QueryResults::Graph(rows) = evaluator.prepare(&construct).execute(&data).unwrap() else {
        panic!("expected strict CONSTRUCT")
    };
    let rows = rows.map(Result::unwrap).collect::<Vec<_>>();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].subject.is_named_node());
    assert_eq!(data.len(), 3);
}
