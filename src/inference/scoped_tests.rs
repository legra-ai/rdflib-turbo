use std::collections::HashSet;

use oxigraph::model::{
    BlankNode, GeneralizedQuad, GraphName, Literal, NamedNode, NamedOrBlankNode,
};
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;

use super::super::backend::Graph;

const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";

#[test]
fn rdfs_literal_identity_is_published_even_when_present_only_in_source() {
    // bounded: scoped/union reads and the three inference modes.
    for union in [false, true] {
        for mode in ["rdfs", "owlrl", "both"] {
            let store = Store::new().unwrap();
            let source: GraphName = iri("urn:source").into();
            let destination: GraphName = iri("urn:destination").into();
            let input = GeneralizedQuad::new(
                iri("urn:s"),
                iri("urn:label"),
                Literal::new_simple_literal("label"),
                source.clone(),
            );
            let expected = GeneralizedQuad::new(
                input.subject.clone(),
                input.predicate.clone(),
                input.object.clone(),
                destination.clone(),
            );
            let mut transaction = store.start_transaction().unwrap();
            transaction.insert_generalized(&input);
            super::expand(
                &mut transaction,
                mode,
                (!union).then_some(source),
                destination,
                super::super::InferenceRequest::default(),
            )
            .unwrap();
            assert_eq!(
                transaction.contains_generalized(&expected).unwrap(),
                mode == "rdfs",
                "union={union}, mode={mode}"
            );
            assert!(transaction.contains_generalized(&input).unwrap());
        }
    }
}

#[test]
fn legacy_dataset_destination_and_delta_include_literal_identity_publication() {
    // bounded: the two legacy GeneralizedDataset entry points and two source quads.
    for delta in [false, true] {
        let source: GraphName = iri("urn:source").into();
        let destination: GraphName = iri("urn:destination").into();
        let original = GeneralizedQuad::new(
            iri("urn:s"),
            iri("urn:label"),
            Literal::new_simple_literal("label"),
            source.clone(),
        );
        let publication = GeneralizedQuad::new(
            original.subject.clone(),
            original.predicate.clone(),
            original.object.clone(),
            destination.clone(),
        );
        let mut data = oxigraph::model::GeneralizedDataset::from_iter([
            original.clone(),
            quad("urn:s", TYPE, "urn:C", &source),
        ]);
        if delta {
            let (additions, _) = super::super::expand_delta(
                &mut data,
                "rdfs",
                Some(destination.clone()),
                super::super::InferenceRequest::default(),
            )
            .unwrap();
            assert!(additions.contains(&publication));
            assert!(!additions.contains(&original));
            assert!(
                super::super::expand_delta(
                    &mut data,
                    "rdfs",
                    Some(destination.clone()),
                    super::super::InferenceRequest::default()
                )
                .unwrap()
                .0
                .iter()
                .all(|quad| quad != publication.as_ref())
            );
        } else {
            super::super::expand_into(
                &mut data,
                "rdfs",
                destination.clone(),
                super::super::InferenceRequest::default(),
            )
            .unwrap();
        }
        assert!(data.contains(&original));
        assert!(data.contains(&publication));
        assert!(!data.contains(&quad("urn:s", TYPE, "urn:C", &destination)));
    }
}

fn iri(value: &str) -> NamedNode {
    NamedNode::new(value).unwrap()
}

fn quad(subject: &str, predicate: &str, object: &str, graph: &GraphName) -> GeneralizedQuad {
    GeneralizedQuad::new(iri(subject), iri(predicate), iri(object), graph.clone())
}

#[test]
fn default_named_and_blank_sources_exclude_outside_schema() {
    // bounded: three graph kinds and three source triples per fixture.
    for source in [
        GraphName::DefaultGraph,
        iri("urn:source").into(),
        BlankNode::new("source").unwrap().into(),
    ] {
        let store = Store::new().unwrap();
        let outside = iri("urn:outside").into();
        let mut transaction = store.start_transaction().unwrap();
        transaction.insert_generalized(&quad("urn:x", TYPE, "urn:C", &source));
        transaction.insert_generalized(&quad("urn:C", SUBCLASS, "urn:D", &source));
        transaction.insert_generalized(&quad("urn:D", SUBCLASS, "urn:Outside", &outside));
        super::expand(
            &mut transaction,
            "rdfs",
            Some(source.clone()),
            source.clone(),
            super::super::InferenceRequest::default(),
        )
        .unwrap();
        assert!(
            transaction
                .contains_generalized(&quad("urn:x", TYPE, "urn:D", &source))
                .unwrap()
        );
        assert!(
            !transaction
                .contains_generalized(&quad("urn:x", TYPE, "urn:Outside", &source))
                .unwrap()
        );
    }
}

#[test]
fn separate_destination_terminates_without_becoming_a_rule_source() {
    let store = Store::new().unwrap();
    let source = iri("urn:source").into();
    let destination: GraphName = BlankNode::new("destination").unwrap().into();
    let mut transaction = store.start_transaction().unwrap();
    transaction.insert_generalized(&quad("urn:x", TYPE, "urn:C", &source));
    transaction.insert_generalized(&quad("urn:C", SUBCLASS, "urn:D", &source));
    transaction.insert_generalized(&quad("urn:D", SUBCLASS, "urn:E", &source));
    super::expand(
        &mut transaction,
        "rdfs",
        Some(source.clone()),
        destination.clone(),
        super::super::InferenceRequest::default(),
    )
    .unwrap();
    assert!(
        transaction
            .contains_generalized(&quad("urn:x", TYPE, "urn:D", &destination))
            .unwrap()
    );
    assert!(
        !transaction
            .contains_generalized(&quad("urn:x", TYPE, "urn:E", &destination))
            .unwrap()
    );
    assert!(
        !transaction
            .contains_generalized(&quad("urn:x", TYPE, "urn:D", &source))
            .unwrap()
    );
}

#[test]
fn union_source_keeps_existing_store_inference_behavior() {
    let store = Store::new().unwrap();
    let schema = iri("urn:schema").into();
    let data = iri("urn:data").into();
    let destination = GraphName::DefaultGraph;
    let mut transaction = store.start_transaction().unwrap();
    transaction.insert_generalized(&quad("urn:x", TYPE, "urn:C", &data));
    transaction.insert_generalized(&quad("urn:C", SUBCLASS, "urn:D", &schema));
    super::expand(
        &mut transaction,
        "rdfs",
        None,
        destination.clone(),
        super::super::InferenceRequest::default(),
    )
    .unwrap();
    assert!(
        transaction
            .contains_generalized(&quad("urn:x", TYPE, "urn:D", &destination))
            .unwrap()
    );
    assert!(
        !transaction
            .contains_generalized(&quad("urn:x", TYPE, "urn:C", &destination))
            .unwrap()
    );
}

#[test]
fn configured_scopes_initialize_axioms_only_at_destination_and_preserve_noops() {
    for mode in ["rdfs", "owlrl", "both", "none", ""] {
        let store = Store::new().unwrap();
        let source: GraphName = iri("urn:source").into();
        let destination: GraphName = iri("urn:destination").into();
        let mut transaction = store.start_transaction().unwrap();
        transaction.insert_generalized(&quad("urn:x", TYPE, "urn:C", &source));
        let request = super::super::InferenceOptions {
            axiomatic_triples: true,
            datatype_axioms: true,
        }
        .into();
        super::expand(
            &mut transaction,
            mode,
            Some(source.clone()),
            destination.clone(),
            request,
        )
        .unwrap();
        assert_eq!(
            transaction
                .quads_for_pattern(None, None, None, Some(source.as_ref()))
                .count(),
            1
        );
        assert_eq!(
            transaction
                .quads_for_pattern(None, None, None, Some(destination.as_ref()))
                .count()
                > 0,
            !matches!(mode, "none" | "")
        );
    }
}

#[test]
fn bounded_equality_witnesses_never_read_destination_edges_in_the_same_round() {
    const SAME: &str = "http://www.w3.org/2002/07/owl#sameAs";
    for mixed in [false, true] {
        let store = Store::new().unwrap();
        let source: GraphName = iri("urn:source").into();
        let destination: GraphName = iri("urn:destination").into();
        let nodes: Vec<NamedOrBlankNode> = (0..8)
            .map(|index| {
                if mixed && index == 1 {
                    BlankNode::new("member").unwrap().into()
                } else {
                    iri(&format!("urn:member:{index}")).into()
                }
            })
            .collect();
        let mut transaction = store.start_transaction().unwrap();
        // Two closed four-node cliques linked by one bridge. Rows 0/7 have
        // degree4 in the completed eight-node component and are not majorities.
        for offset in [0, 4] {
            for left in offset..offset + 4 {
                for right in offset..offset + 4 {
                    transaction.insert_generalized(&GeneralizedQuad::new(
                        nodes[left].clone(),
                        iri(SAME),
                        nodes[right].clone(),
                        source.clone(),
                    ));
                }
            }
        }
        for (left, right) in [(3, 4), (4, 3)] {
            transaction.insert_generalized(&GeneralizedQuad::new(
                nodes[left].clone(),
                iri(SAME),
                nodes[right].clone(),
                source.clone(),
            ));
        }
        if mixed {
            transaction.insert_generalized(&GeneralizedQuad::new(
                nodes[6].clone(),
                iri(SAME),
                Literal::new_simple_literal("literal"),
                source.clone(),
            ));
            transaction.insert_generalized(&GeneralizedQuad::new(
                nodes[0].clone(),
                iri("urn:p"),
                Literal::new_simple_literal("ordinary"),
                source.clone(),
            ));
        }
        // Only the destination contains this second hop. It must not complete
        // the SOURCE relation's 0 -> 3 -> 7 transitive witness.
        transaction.insert_generalized(&GeneralizedQuad::new(
            nodes[3].clone(),
            iri(SAME),
            nodes[7].clone(),
            destination.clone(),
        ));
        let view = super::ScopedGraph {
            transaction: &mut transaction,
            source,
            destination,
        };
        assert_eq!(
            view.quads(
                Some(nodes[0].as_ref().into()),
                Some(iri(SAME).as_ref().into()),
                None
            )
            .count(),
            4
        );
        let mut expected = HashSet::new();
        for (head, body) in [
            ("?y owl:sameAs ?x", "?x owl:sameAs ?y"),
            ("?x owl:sameAs ?z", "?x owl:sameAs ?y . ?y owl:sameAs ?z"),
            ("?y ?p ?o", "?x owl:sameAs ?y . ?x ?p ?o"),
            ("?s ?p ?y", "?x owl:sameAs ?y . ?s ?p ?x"),
        ] {
            let query = SparqlEvaluator::new().parse_query(&format!("PREFIX owl:<http://www.w3.org/2002/07/owl#> CONSTRUCT {{ {head} }} WHERE {{ {body} }}")).unwrap();
            let QueryResults::Graph(triples) = view.query(query).unwrap() else {
                panic!("expected graph")
            };
            for triple in triples {
                let triple = triple.unwrap();
                let quad = GeneralizedQuad::new(
                    triple.subject,
                    triple.predicate,
                    triple.object,
                    GraphName::DefaultGraph,
                );
                if !view.contains((&quad).into()).unwrap() {
                    expected.insert(quad);
                }
            }
        }
        let mut actual = super::super::Pending::default();
        super::super::same_as::apply(&view, &mut actual).unwrap();
        let generalized = super::super::same_as_reference_tests::one_round(&view).unwrap();
        assert!(
            expected.is_subset(&generalized),
            "strict public CONSTRUCT control"
        );
        if !mixed {
            assert_eq!(expected, generalized, "ordinary strict-domain control");
        }
        assert_eq!(actual.0, generalized);
        assert!(!actual.0.contains(&GeneralizedQuad::new(
            nodes[0].clone(),
            iri(SAME),
            nodes[7].clone(),
            GraphName::DefaultGraph
        )));
    }
}
