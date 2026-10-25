//! Programmatic data slots use full terms; graph-name and metadata checks stay strict.
use super::{
    read,
    typed::{QuadInput, TermTuple},
    typed_view::TermView,
};
use oxigraph::{model::*, store::Store};

fn forms() -> [TermTuple; 3] {
    [
        ("uri".into(), "urn:role".into(), None, None),
        ("bnode".into(), "role".into(), None, None),
        ("literal".into(), "role".into(), None, Some("en".into())),
    ]
}
fn value(tuple: &TermTuple) -> TermView<'_> {
    TermView::new(&tuple.0, &tuple.1, tuple.2.as_deref(), tuple.3.as_deref())
}

#[test]
fn typed_data_insertion_retains_all_subject_and_predicate_roles() {
    for subject in forms() {
        for predicate in forms() {
            let store = Store::new().unwrap();
            let mut transaction = store.start_transaction().unwrap();
            let object = ("literal".into(), "value".into(), None, None);
            let quad = QuadInput::new(subject.clone(), predicate.clone(), object, None).quad();
            assert!(
                quad.is_ok(),
                "programmatic terms must not be narrowed: {quad:?}"
            );
            transaction.insert_generalized(quad.unwrap().as_ref());
            transaction.commit().unwrap();
            assert_eq!(store.len().unwrap(), 1);
        }
    }
}

#[test]
fn full_term_read_membership_and_lexical_scans_keep_role_selectors() {
    for subject in forms() {
        for predicate in forms() {
            let object = ("literal".into(), "value".into(), None, None);
            let q = GeneralizedQuad::new(
                super::typed::term(subject.clone()).unwrap(),
                super::typed::term(predicate.clone()).unwrap(),
                Literal::new_simple_literal("value"),
                GraphName::DefaultGraph,
            );
            let store = Store::new().unwrap();
            store.insert_generalized(&q).unwrap();
            assert!(
                read::contains_exact(
                    &store,
                    value(&subject),
                    value(&predicate),
                    value(&object),
                    None
                )
                .unwrap(),
                "present role combination was silently treated as absent"
            );
            let (rows, cursor) = read::start(
                &store,
                Some(value(&subject)),
                Some(value(&predicate)),
                Some(value(&object)),
                None,
                7,
            )
            .unwrap();
            assert_eq!(rows.len(), 1);
            assert!(cursor.is_none());
            assert!(rows[0].0.is_some() && rows[0].1.is_some() && rows[0].2.is_some());
            let (rows, cursor) = read::start(&store, None, None, None, None, 7).unwrap();
            assert_eq!(rows.len(), 1);
            assert!(cursor.is_none());
        }
    }
}

#[test]
fn full_term_data_does_not_accept_literal_graph_names_or_invalid_metadata() {
    let [subject, _, literal] = forms();
    assert!(
        QuadInput::new(
            subject.clone(),
            literal.clone(),
            literal.clone(),
            Some(literal.clone())
        )
        .quad()
        .is_err()
    );
    let invalid = (
        "literal".into(),
        "bad".into(),
        Some("urn:datatype".into()),
        Some("en".into()),
    );
    for (s, p, o) in [
        (invalid.clone(), literal.clone(), literal.clone()),
        (subject.clone(), invalid.clone(), literal.clone()),
        (subject, literal, invalid),
    ] {
        assert!(QuadInput::new(s, p, o, None).quad().is_err());
    }
}
