//! Full-Term projection retains the existing lexical owners and strict controls.
use super::super::LexicalTerm;
use super::*;
use crate::model::{BlankNode, GraphName, Literal, NamedNode, Term};

fn owned(term: LexicalTerm) -> Term {
    match term {
        LexicalTerm::NamedNode(value) => NamedNode::new_unchecked(value.as_str()).into(),
        LexicalTerm::BlankNode(value) => BlankNode::new_unchecked(value.as_str()).into(),
        LexicalTerm::Literal {
            value,
            datatype,
            language,
            ..
        } => match (datatype, language) {
            (_, Some(language)) => {
                Literal::new_language_tagged_literal_unchecked(value.as_str(), language.as_str())
                    .into()
            }
            (Some(datatype), None) => Literal::new_typed_literal(
                value.as_str(),
                NamedNode::new_unchecked(datatype.as_str()),
            )
            .into(),
            (None, None) => Literal::new_simple_literal(value.as_str()).into(),
        },
        #[cfg(feature = "rdf-12")]
        LexicalTerm::Triple(_) => panic!("fixed fixture contains no quoted triple"),
    }
}

#[test]
fn generalized_lexical_projection_keeps_every_role_mask_and_strict_projection_errors() {
    let roles: [Term; 3] = [
        NamedNode::new_unchecked("urn:term").into(),
        BlankNode::new_unchecked("term").into(),
        Literal::new_language_tagged_literal_unchecked("literal\0é", "en").into(),
    ];
    for subject in &roles {
        for predicate in &roles {
            let store = Store::new().unwrap();
            let object: Term =
                Literal::new_typed_literal("object\0é", NamedNode::new_unchecked("urn:datatype"))
                    .into();
            let graph = GraphName::from(BlankNode::new_unchecked("scope"));
            store
                .insert_generalized(GeneralizedQuadRef {
                    subject: subject.as_ref(),
                    predicate: predicate.as_ref(),
                    object: object.as_ref(),
                    graph_name: graph.as_ref(),
                })
                .unwrap();
            for mask in 0..8 {
                let mut cursor = store
                    .generalized_quads_for_pattern(
                        Some(subject.as_ref()),
                        Some(predicate.as_ref()),
                        Some(object.as_ref()),
                        Some(graph.as_ref()),
                    )
                    .project_lexical(mask & 1 != 0, mask & 2 != 0, mask & 4 != 0);
                let row = cursor.next().unwrap().unwrap().map(|v| v.map(owned));
                assert_eq!(
                    row,
                    [
                        Some(subject.clone()).filter(|_| mask & 1 != 0),
                        Some(predicate.clone()).filter(|_| mask & 2 != 0),
                        Some(object.clone()).filter(|_| mask & 4 != 0)
                    ]
                );
                assert!(cursor.next().is_none());
            }
            let strict = store
                .quads_for_pattern(None, None, None, Some(graph.as_ref()))
                .project_lexical(true, true, true)
                .next()
                .unwrap();
            assert_eq!(
                strict.is_err(),
                subject.is_literal() || !predicate.is_named_node()
            );
        }
    }
}

#[test]
fn generalized_projected_words_outlive_owner_and_keep_pinned_committed_version() {
    let store = Store::new().unwrap();
    let long = "shared long lexical é".repeat(1024);
    let subject: Term = Literal::new_simple_literal(&long).into();
    let predicate: Term = BlankNode::new_unchecked(format!("p{long}")).into();
    let object: Term = Literal::new_language_tagged_literal_unchecked(&long, "en").into();
    let q = GeneralizedQuadRef {
        subject: subject.as_ref(),
        predicate: predicate.as_ref(),
        object: object.as_ref(),
        graph_name: GraphNameRef::DefaultGraph,
    };
    store.insert_generalized(q).unwrap();
    let mut cursor = store
        .generalized_quads_for_pattern(None, None, None, None)
        .project_lexical(true, true, true);
    store.remove_generalized(q).unwrap();
    let row = cursor.next().unwrap().unwrap();
    drop(cursor);
    drop(store);
    assert_eq!(
        row.map(|v| v.map(owned)),
        [Some(subject), Some(predicate), Some(object)]
    );
}

#[test]
fn encoded_union_projection_deduplicates_full_terms_on_one_pinned_reader() {
    let store = Store::new().unwrap();
    let values = [
        Literal::new_simple_literal("long lexical word ".repeat(300)),
        Literal::new_typed_literal(
            "long lexical word ".repeat(300),
            NamedNode::new_unchecked("urn:datatype"),
        ),
    ];
    let graphs = [
        GraphName::DefaultGraph,
        NamedNode::new("urn:scope").unwrap().into(),
        BlankNode::new("urn:scope").unwrap().into(),
    ];
    for object in &values {
        for graph in &graphs {
            store
                .insert_generalized(&GeneralizedQuad::new(
                    Literal::new_simple_literal("subject"),
                    BlankNode::new("predicate").unwrap(),
                    object.clone(),
                    graph.clone(),
                ))
                .unwrap();
        }
    }
    let mut union = store
        .read_snapshot()
        .generalized_quads_for_pattern(None, None, None, None)
        .unique_spo()
        .project_lexical(true, true, true);
    store
        .insert_generalized(&GeneralizedQuad::new(
            NamedNode::new("urn:late").unwrap(),
            NamedNode::new("urn:p").unwrap(),
            NamedNode::new("urn:o").unwrap(),
            GraphName::DefaultGraph,
        ))
        .unwrap();
    let rows: Vec<_> = union.by_ref().map(Result::unwrap).collect();
    assert_eq!(rows.len(), 2);
    let observed: std::collections::HashSet<_> = rows
        .into_iter()
        .map(|row| row[2].clone().map(owned).unwrap())
        .collect();
    assert_eq!(observed, values.into_iter().map(Term::from).collect());
    assert!(union.next().is_none());
    assert_eq!(
        store
            .generalized_quads_for_pattern(None, None, None, None)
            .count(),
        7
    );
}

#[test]
fn union_row_contexts_pin_the_same_version_and_outlive_outer_cursor_and_store() {
    let store = Store::new().unwrap();
    let subject = Literal::new_simple_literal("subject");
    let predicate = BlankNode::new("predicate").unwrap();
    let object = NamedNode::new("urn:object").unwrap();
    let graphs = [
        GraphName::DefaultGraph,
        NamedNode::new("urn:scope").unwrap().into(),
        BlankNode::new("urn:scope").unwrap().into(),
    ];
    for graph in &graphs {
        store
            .insert_generalized(&GeneralizedQuad::new(
                subject.clone(),
                predicate.clone(),
                object.clone(),
                graph.clone(),
            ))
            .unwrap();
    }
    // A graph selector chooses statements, while their context iterators expose
    // every physical occurrence, matching RDFLib Memory.triples(context=...).
    for graph in &graphs {
        let mut selected = store
            .read_snapshot()
            .generalized_quads_for_pattern(None, None, None, Some(graph.as_ref()))
            .project_union_lexical();
        let row = selected.next().unwrap().unwrap();
        assert!(selected.next().is_none());
        assert_eq!(row.contexts.count(), graphs.len());
    }
    let mut outer = store
        .read_snapshot()
        .generalized_quads_for_pattern(None, None, None, None)
        .project_union_lexical();
    let row = outer.next().unwrap().unwrap();
    drop(outer);
    store
        .remove_generalized(&GeneralizedQuad::new(
            subject,
            predicate,
            object,
            graphs[1].clone(),
        ))
        .unwrap();
    drop(store);
    let actual: std::collections::HashSet<_> = row
        .contexts
        .map(|graph| match graph.unwrap() {
            LexicalGraphName::DefaultGraph => GraphName::DefaultGraph,
            LexicalGraphName::NamedNode(text) => NamedNode::new_unchecked(text.as_str()).into(),
            LexicalGraphName::BlankNode(text) => BlankNode::new_unchecked(text.as_str()).into(),
        })
        .collect();
    assert_eq!(actual, graphs.into_iter().collect());
}
