//! Exact catalogue optimizations retain arbitrary-clause and scoped publication semantics.
use super::*;
use oxigraph::model::{GeneralizedQuad, GraphName, Literal as RdfLiteral, NamedNode, Triple};
use oxigraph::sparql::QueryResults;
use oxrdf::GeneralizedDataset;

const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const EQUIVALENT: &str = "http://www.w3.org/2002/07/owl#equivalentClass";

#[test]
fn catalogue_compiles_identical_bodies_once() {
    let actual =
        ["rdfs", "owlrl", "both"].map(|mode| CompiledRule::compiled_for_mode(mode).unwrap().len());
    assert_eq!(actual, [11, 43, 49]);
}

fn iri(value: &str) -> NamedNode {
    NamedNode::new(value.to_owned()).unwrap()
}

fn results(rule: CompiledRule, data: &GeneralizedDataset) -> Vec<Triple> {
    let QueryResults::Graph(rows) = rule
        .into_query()
        .on_queryable_dataset(data)
        .execute()
        .unwrap()
    else {
        panic!("expected graph")
    };
    rows.map(|row| row.unwrap()).collect()
}

fn assert_self_membership_elided(size: usize) {
    let mut data = GeneralizedDataset::new();
    data.insert(&GeneralizedQuad::new(
        iri("urn:C"),
        iri(EQUIVALENT),
        iri("urn:C"),
        GraphName::DefaultGraph,
    ));
    for index in 0..size {
        data.insert(&GeneralizedQuad::new(
            iri(&format!("urn:s:{index}")),
            iri(TYPE),
            iri("urn:C"),
            GraphName::DefaultGraph,
        ));
    }
    for id in [CaxEqc1, CaxEqc2] {
        assert_eq!(
            results(CompiledRule::try_new(&id.definition()).unwrap(), &data).len(),
            size
        );
        assert_eq!(
            results(CompiledRule::from_catalogue(id).unwrap(), &data).len(),
            0
        );
    }
}

#[test]
fn catalogue_self_membership_32_has_zero_output_rows() {
    assert_self_membership_elided(32);
}

#[test]
fn catalogue_self_membership_128_has_zero_output_rows() {
    assert_self_membership_elided(128);
}

#[test]
fn catalogue_nonself_memberships_remain_bidirectional_and_scope_selected() {
    for selected in [
        GraphName::DefaultGraph,
        iri("urn:selected").into(),
        oxigraph::model::BlankNode::new("selected").unwrap().into(),
    ] {
        let hidden: GraphName = iri("urn:hidden").into();
        let mut data = GeneralizedDataset::new();
        for (subject, predicate, object) in [
            ("urn:A", EQUIVALENT, "urn:B"),
            ("urn:A", EQUIVALENT, "urn:A"),
            ("urn:B", EQUIVALENT, "urn:B"),
            ("urn:first", TYPE, "urn:A"),
            ("urn:second", TYPE, "urn:B"),
        ] {
            data.insert(&GeneralizedQuad::new(
                iri(subject),
                iri(predicate),
                iri(object),
                selected.clone(),
            ));
        }
        data.insert(&GeneralizedQuad::new(
            iri("urn:A"),
            iri(EQUIVALENT),
            iri("urn:Hidden"),
            hidden,
        ));
        for (id, subject, class) in [
            (CaxEqc1, "urn:first", "urn:B"),
            (CaxEqc2, "urn:second", "urn:A"),
        ] {
            let mut query = CompiledRule::from_catalogue(id).unwrap().into_query();
            query
                .dataset_mut()
                .set_default_graph(vec![selected.clone()]);
            let QueryResults::Graph(rows) = query.on_queryable_dataset(&data).execute().unwrap()
            else {
                panic!("expected graph")
            };
            let actual: Vec<_> = rows.map(|row| row.unwrap()).collect();
            assert_eq!(
                actual,
                vec![Triple::new(iri(subject), iri(TYPE), iri(class))]
            );
        }
    }
}

#[test]
fn catalogue_self_elision_preserves_separate_destination_and_union_contracts() {
    for source in [
        GraphName::DefaultGraph,
        iri("urn:source").into(),
        oxigraph::model::BlankNode::new("source").unwrap().into(),
    ] {
        for mode in ["owlrl", "both"] {
            for union in [false, true] {
                for separate in [false, true] {
                    let store = oxigraph::store::Store::new().unwrap();
                    let destination = if separate {
                        iri("urn:destination").into()
                    } else {
                        source.clone()
                    };
                    let mut transaction = store.start_transaction().unwrap();
                    for (subject, predicate, object) in [
                        ("urn:A", EQUIVALENT, "urn:B"),
                        ("urn:C", EQUIVALENT, "urn:C"),
                        ("urn:first", TYPE, "urn:A"),
                        ("urn:second", TYPE, "urn:B"),
                        ("urn:self", TYPE, "urn:C"),
                    ] {
                        transaction.insert_generalized(&GeneralizedQuad::new(
                            iri(subject),
                            iri(predicate),
                            iri(object),
                            source.clone(),
                        ));
                    }
                    transaction.insert_generalized(&GeneralizedQuad::new(
                        iri("urn:A"),
                        iri(EQUIVALENT),
                        iri("urn:Hidden"),
                        iri("urn:hidden"),
                    ));
                    super::super::scoped::expand(
                        &mut transaction,
                        mode,
                        (!union).then_some(source.clone()),
                        destination.clone(),
                        super::super::InferenceRequest::default(),
                    )
                    .unwrap();
                    for (subject, class) in [("urn:first", "urn:B"), ("urn:second", "urn:A")] {
                        assert!(
                            transaction
                                .contains_generalized(&GeneralizedQuad::new(
                                    iri(subject),
                                    iri(TYPE),
                                    iri(class),
                                    destination.clone()
                                ))
                                .unwrap()
                        );
                    }
                    assert_eq!(
                        transaction
                            .contains_generalized(&GeneralizedQuad::new(
                                iri("urn:self"),
                                iri(TYPE),
                                iri("urn:C"),
                                destination.clone()
                            ))
                            .unwrap(),
                        !separate
                    );
                    assert_eq!(
                        transaction
                            .contains_generalized(&GeneralizedQuad::new(
                                iri("urn:first"),
                                iri(TYPE),
                                iri("urn:Hidden"),
                                destination
                            ))
                            .unwrap(),
                        union
                    );
                }
            }
        }
    }
}

#[test]
fn catalogue_guard_compares_exact_lexical_terms_not_numeric_values() {
    let store = oxigraph::store::Store::new().unwrap();
    let integer = iri("http://www.w3.org/2001/XMLSchema#integer");
    let left = RdfLiteral::new_typed_literal("01", integer.clone());
    let right = RdfLiteral::new_typed_literal("1", integer);
    let mut transaction = store.start_transaction().unwrap();
    transaction.insert_generalized(&GeneralizedQuad::new(
        left.clone(),
        iri(EQUIVALENT),
        right.clone(),
        GraphName::DefaultGraph,
    ));
    transaction.insert_generalized(&GeneralizedQuad::new(
        iri("urn:first"),
        iri(TYPE),
        left.clone(),
        GraphName::DefaultGraph,
    ));
    transaction.insert_generalized(&GeneralizedQuad::new(
        iri("urn:second"),
        iri(TYPE),
        right.clone(),
        GraphName::DefaultGraph,
    ));
    for (id, subject, object) in [(CaxEqc1, "urn:first", right), (CaxEqc2, "urn:second", left)] {
        let QueryResults::Graph(rows) = CompiledRule::from_catalogue(id)
            .unwrap()
            .into_query()
            .on_transaction(&transaction)
            .execute()
            .unwrap()
        else {
            panic!("expected graph")
        };
        let actual: Vec<_> = rows.map(|row| row.unwrap()).collect();
        assert_eq!(actual, vec![Triple::new(iri(subject), iri(TYPE), object)]);
    }
}

#[test]
fn catalogue_optimizer_rejects_changed_shapes_and_keeps_generic_clauses_faithful() {
    for id in [CaxEqc1, CaxEqc2] {
        let original = id.definition();
        let mut changed = original.clone();
        changed.body.pop();
        assert!(CompiledRule::without_self_membership(id, &changed).is_err());
        changed = original.clone();
        changed.head.terms[1] = changed.head.terms[2].clone();
        assert!(CompiledRule::without_self_membership(id, &changed).is_err());
        changed = original.clone();
        let Literal::Positive(atom) = &mut changed.body[0] else {
            panic!("positive catalogue body")
        };
        atom.terms.swap(0, 2);
        assert!(CompiledRule::without_self_membership(id, &changed).is_err());
        assert!(CompiledRule::without_self_membership(Rdf1, &original).is_err());
    }
}
