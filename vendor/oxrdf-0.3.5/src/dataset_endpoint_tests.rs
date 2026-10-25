use crate::*;
use std::collections::HashSet;
fn quad(subject: NamedOrBlankNode, object: Term, graph: GraphName) -> Quad {
    Quad::new(subject, NamedNode::new_unchecked("urn:p"), object, graph)
}
fn ordinary_subject() -> NamedOrBlankNode {
    NamedNode::new_unchecked("urn:s").into()
}
fn ordinary_object() -> Term {
    NamedNode::new_unchecked("urn:o").into()
}
#[test]
fn zero_numeric_id_round_trips_exact_identity() {
    let q = quad(
        BlankNode::new_from_unique_id(0).into(),
        ordinary_object(),
        GraphName::DefaultGraph,
    );
    let d = Dataset::from_iter([q.clone()]);
    assert_eq!(d.iter().next().unwrap(), q.as_ref());
}
#[test]
fn maximum_id_subject_range_matches_stored_quad() {
    let n = BlankNode::new_from_unique_id(u128::MAX);
    let q = quad(n.clone().into(), ordinary_object(), GraphName::DefaultGraph);
    let d = Dataset::from_iter([q]);
    assert_eq!(d.quads_for_subject(&n).count(), 1);
}
#[test]
fn maximum_id_object_range_matches_stored_quad() {
    let n = BlankNode::new_from_unique_id(u128::MAX);
    let q = quad(
        ordinary_subject(),
        n.clone().into(),
        GraphName::DefaultGraph,
    );
    let d = Dataset::from_iter([q]);
    assert_eq!(d.quads_for_object(&n).count(), 1);
}
#[test]
fn maximum_id_graph_range_matches_stored_quad() {
    let n = BlankNode::new_from_unique_id(u128::MAX);
    let q = quad(ordinary_subject(), ordinary_object(), n.clone().into());
    let d = Dataset::from_iter([q]);
    assert_eq!(d.quads_for_graph_name(&n).count(), 1);
}
#[test]
fn maximum_id_exact_membership_does_find_stored_quad() {
    let n = BlankNode::new_from_unique_id(u128::MAX);
    let q = quad(n.clone().into(), n.clone().into(), n.into());
    let d = Dataset::from_iter([q.clone()]);
    assert!(d.contains(&q));
}

#[test]
fn zero_and_max_constructor_equality_contract_is_preserved() {
    let numeric_zero = BlankNode::new_from_unique_id(0);
    let named_zero = BlankNode::new("0").unwrap();
    assert_ne!(numeric_zero, named_zero);
    assert_eq!(numeric_zero.as_str(), named_zero.as_str());
    assert_ne!(numeric_zero.as_ref(), named_zero.as_ref());
    let maximum = BlankNode::new_from_unique_id(u128::MAX);
    assert_eq!(BlankNode::new(maximum.as_str()).unwrap(), maximum);
}
#[test]
fn endpoint_terms_keep_exact_identity_through_every_dataset_and_graph_mask() {
    let maximum = BlankNode::new_from_unique_id(u128::MAX);
    let nodes = [
        BlankNode::new_from_unique_id(0),
        BlankNode::new("0").unwrap(),
        BlankNode::new_from_unique_id(1),
        maximum.clone(),
        BlankNode::new(maximum.as_str()).unwrap(),
        BlankNode::new_from_unique_id(u128::MAX - 1),
        BlankNode::new("labelled").unwrap(),
    ];
    let mut expected = HashSet::new();
    for s in &nodes {
        for p in ["urn:p", "urn:q"] {
            for o in nodes.iter().map(|n| Term::from(n.clone())).chain([
                Term::from(Literal::new_simple_literal("0")),
                Term::from(Literal::new_simple_literal(maximum.as_str())),
            ]) {
                for g in nodes
                    .iter()
                    .map(|n| GraphName::from(n.clone()))
                    .chain([GraphName::DefaultGraph])
                {
                    expected.insert(Quad::new(
                        s.clone(),
                        NamedNode::new_unchecked(p),
                        o.clone(),
                        g,
                    ));
                }
            }
        }
    }
    let data = Dataset::from_iter(expected.iter().map(Quad::as_ref));
    let observed: HashSet<_> = data.iter().map(QuadRef::into_owned).collect();
    assert!(
        observed == expected,
        "stored endpoint identities differ from supplied public values"
    );
    for q in &expected {
        for mask in 0..16 {
            let s = (mask & 1 != 0).then_some(q.subject.as_ref());
            let p = (mask & 2 != 0).then_some(q.predicate.as_ref());
            let o = (mask & 4 != 0).then_some(q.object.as_ref());
            let g = (mask & 8 != 0).then_some(q.graph_name.as_ref());
            let actual: HashSet<_> = data
                .quads_for_pattern(s, p, o, g)
                .map(QuadRef::into_owned)
                .collect();
            let wanted: HashSet<_> = expected
                .iter()
                .filter(|q| {
                    s.is_none_or(|s| s == q.subject.as_ref())
                        && p.is_none_or(|p| p == q.predicate.as_ref())
                        && o.is_none_or(|o| o == q.object.as_ref())
                        && g.is_none_or(|g| g == q.graph_name.as_ref())
                })
                .cloned()
                .collect();
            assert!(actual == wanted, "datasetmask={mask},target={q:?}");
        }
        let graph = data.graph(q.graph_name.as_ref());
        for mask in 0..8 {
            let s = (mask & 1 != 0).then_some(q.subject.as_ref());
            let p = (mask & 2 != 0).then_some(q.predicate.as_ref());
            let o = (mask & 4 != 0).then_some(q.object.as_ref());
            let actual: HashSet<_> = graph
                .triples_for_pattern(s, p, o)
                .map(TripleRef::into_owned)
                .collect();
            let wanted: HashSet<_> = expected
                .iter()
                .filter(|t| {
                    t.graph_name == q.graph_name
                        && s.is_none_or(|s| s == t.subject.as_ref())
                        && p.is_none_or(|p| p == t.predicate.as_ref())
                        && o.is_none_or(|o| o == t.object.as_ref())
                })
                .map(|t| Triple::new(t.subject.clone(), t.predicate.clone(), t.object.clone()))
                .collect();
            assert!(actual == wanted, "graphmask={mask},target={q:?}");
        }
    }
}
