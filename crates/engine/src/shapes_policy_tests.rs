use super::*;
use std::collections::HashSet;

use oxigraph::store::Store;

use crate::IndexedShapesSource;

#[test]
fn numeric_boolean_projection_preserves_exact_bound_patterns_and_source_rows() {
    let store = Store::new().unwrap();
    let shape: Term = Literal::new_simple_literal("shape").into();
    let one: Term = Literal::new_typed_literal("1", iri(XSD, "boolean")).into();
    let zero: Term = Literal::from(false).into();
    let make = |predicate: Term, object: Term| {
        GeneralizedQuad::new(shape.clone(), predicate, object, GraphName::DefaultGraph)
    };
    // bounded: six physical rows and five projected rows in this fixture.
    let physical = [
        make(iri(SH, "closed").into(), one.clone()),
        make(iri(SH, "closed").into(), zero.clone()),
        make(iri(SH, "uniqueLang").into(), one.clone()),
        make(iri("urn:", "ordinary").into(), one.clone()),
        make(BlankNode::new("annotation").unwrap().into(), one.clone()),
        make(iri(SH, "deactivated").into(), Literal::from(true).into()),
    ];
    for row in &physical {
        store.insert_generalized(row).unwrap();
    }
    let source = IndexedShapesSource::new(store.read_snapshot(), Some(GraphName::DefaultGraph));
    let view = ShapesPolicy::new(&source, false);
    let mut expected: HashSet<_> = physical.iter().cloned().collect();
    expected.remove(&physical[0]);
    expected.remove(&physical[2]);
    expected.insert(make(iri(SH, "uniqueLang").into(), zero.clone()));
    let actual: HashSet<_> = view
        .matching_quads(None, None, None, None)
        .collect::<Result<_>>()
        .unwrap();
    assert_eq!(actual, expected);
    // Check every mask, including a physical "1" which disappears only for
    // designated roles and false objects which gain a projected input stream.
    for witness in physical.iter().chain(expected.iter()) {
        for mask in 0..16 {
            let subject = (mask & 1 != 0).then(|| witness.subject.as_ref());
            let predicate = (mask & 2 != 0).then(|| witness.predicate.as_ref());
            let object = (mask & 4 != 0).then(|| witness.object.as_ref());
            let graph = (mask & 8 != 0).then(|| witness.graph_name.as_ref());
            let actual: Vec<_> = view
                .matching_quads(subject, predicate, object, graph)
                .collect::<Result<_>>()
                .unwrap();
            let wanted: HashSet<_> = expected
                .iter()
                .filter(|row| {
                    subject.is_none_or(|value| value == row.subject.as_ref())
                        && predicate.is_none_or(|value| value == row.predicate.as_ref())
                        && object.is_none_or(|value| value == row.object.as_ref())
                        && graph.is_none_or(|value| value == row.graph_name.as_ref())
                })
                .cloned()
                .collect();
            assert_eq!(
                actual.len(),
                wanted.len(),
                "duplicate projected row for mask {mask}"
            );
            assert_eq!(
                actual.into_iter().collect::<HashSet<_>>(),
                wanted,
                "mask {mask}"
            );
        }
    }
    let retained: HashSet<_> = source
        .matching_quads(None, None, None, None)
        .collect::<Result<_>>()
        .unwrap();
    assert_eq!(retained, physical.into_iter().collect());
    let unchanged: HashSet<_> = ShapesPolicy::new(&source, true)
        .matching_quads(None, None, None, None)
        .collect::<Result<_>>()
        .unwrap();
    assert_eq!(unchanged, retained);
}
