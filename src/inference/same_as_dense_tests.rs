//! Dense and sparse equality rows retain exact one-round semantics.
use super::*;

#[test]
fn nearly_closed_components_do_not_enumerate_existing_compositions() {
    for (components, size) in [(1, 16), (4, 16), (1, 32)] {
        let mut data = Dataset::new();
        for component in 0..components {
            for left in 0..size {
                for right in 0..size {
                    if left != right {
                        data.insert(
                            Quad::new(
                                iri(&format!("c{component}n{left}")),
                                same(),
                                iri(&format!("c{component}n{right}")),
                                GraphName::DefaultGraph,
                            )
                            .as_ref(),
                        );
                    }
                }
            }
        }
        let measured = Measured {
            data: &data,
            work: Cell::new(0),
        };
        let mut pending = Pending::default();
        super::super::same_as::apply(&measured, &mut pending).unwrap();
        assert_eq!(pending.0.len(), components * size);
        for component in 0..components {
            for index in 0..size {
                let node = iri(&format!("c{component}n{index}"));
                assert!(pending.0.contains(&Quad::new(
                    node.clone(),
                    same(),
                    node,
                    GraphName::DefaultGraph
                )));
            }
        }
        println!(
            "components={components} size={size} work={} budget={}",
            measured.work.get(),
            20 * components * size * size
        );
        assert!(
            measured.work.get() <= 20 * components * size * size,
            "nearly closed components={components} size={size}: work={} budget={}",
            measured.work.get(),
            20 * components * size * size
        );
    }
}

#[test]
fn dense_and_sparse_rows_match_frozen_round_with_literals_and_disjoint_components() {
    let original = queries(ORIGINAL);
    // bounded: only the fixed equality rule inventory.
    let selected: Vec<_> = OWL_RULES
        .iter()
        .copied()
        .filter(|(template, _)| ORIGINAL.iter().any(|(old, _)| old == template))
        .collect();
    let optimized = queries(&selected);
    for variant in 0..4 {
        let mut data = Dataset::new();
        for left in 0..6 {
            for right in 0..6 {
                if left != right && (variant != 1 || left != 0 || right != 1) {
                    data.insert(
                        Quad::new(
                            iri(&format!("v{left}")),
                            same(),
                            iri(&format!("v{right}")),
                            GraphName::DefaultGraph,
                        )
                        .as_ref(),
                    );
                }
            }
        }
        let blank = BlankNode::new("mixed").unwrap();
        let literal = Literal::new_language_tagged_literal("exact", "en").unwrap();
        for quad in [
            Quad::new(iri("v0"), same(), iri("tail"), GraphName::DefaultGraph),
            Quad::new(iri("tail"), same(), iri("v1"), GraphName::DefaultGraph),
            Quad::new(iri("v2"), same(), blank.clone(), GraphName::DefaultGraph),
            Quad::new(blank, same(), iri("tail"), GraphName::DefaultGraph),
            Quad::new(iri("v3"), same(), literal.clone(), GraphName::DefaultGraph),
            Quad::new(
                iri("disjoint-a"),
                same(),
                iri("disjoint-b"),
                GraphName::DefaultGraph,
            ),
            Quad::new(
                iri("disjoint-b"),
                same(),
                iri("disjoint-c"),
                GraphName::DefaultGraph,
            ),
            Quad::new(iri("tail"), iri("p"), literal, GraphName::DefaultGraph),
            Quad::new(same(), same(), iri("alias"), GraphName::DefaultGraph),
        ] {
            data.insert(quad.as_ref());
        }
        if variant >= 2 {
            data.insert(Quad::new(iri("v4"), same(), iri("v4"), GraphName::DefaultGraph).as_ref());
        }
        if variant == 3 {
            data.insert(
                Quad::new(iri("tail"), same(), iri("v0"), GraphName::DefaultGraph).as_ref(),
            );
        }
        let measured = Measured {
            data: &data,
            work: Cell::new(0),
        };
        let mut actual = Pending::default();
        apply_rules(&measured, &optimized, &mut actual, true).unwrap();
        assert_eq!(
            actual.0,
            reference(&data, &original),
            "mixed dense/sparse variant={variant}"
        );
        assert_eq!(measured.data, &data);
    }
}
