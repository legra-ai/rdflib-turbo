use std::collections::HashSet;

use super::*;
use crate::store_quad;
use oxigraph::store::Store;

#[test]
fn rdf11_patterns_preserve_contexts_filters_and_exact_non_string_terms() {
    let store = Store::new().unwrap();
    let mut transaction = store.start_transaction().unwrap();
    let subject = NamedNode::new("urn:s").unwrap();
    let predicate = NamedNode::new("urn:p").unwrap();
    let plain = Literal::new_simple_literal("value");
    let typed = Literal::new_typed_literal("value", oxrdf::vocab::xsd::STRING);
    let numeric = Literal::new_typed_literal("01", oxrdf::vocab::xsd::INTEGER);
    let contexts = [
        GraphName::DefaultGraph,
        NamedNode::new("urn:named").unwrap().into(),
        BlankNode::new("blank").unwrap().into(),
    ];
    // bounded: seven expected logical quads and ten physical fixture quads.
    let mut expected = Dataset::new();
    for context in &contexts {
        for value in [&plain, &typed, &numeric] {
            transaction.insert(&Quad::new(
                subject.clone(),
                predicate.clone(),
                value.clone(),
                context.clone(),
            ));
        }
        for value in [&typed, &numeric] {
            expected.insert(&Quad::new(
                subject.clone(),
                predicate.clone(),
                value.clone(),
                context.clone(),
            ));
        }
    }
    let outside = Quad::new(
        NamedNode::new("urn:other").unwrap(),
        NamedNode::new("urn:q").unwrap(),
        Literal::new_simple_literal("other"),
        GraphName::DefaultGraph,
    );
    transaction.insert(&outside);
    expected.insert(&store_quad(outside));
    let mut view = Rdf11Transaction::new(&mut transaction);
    for subject_filter in [None, Some(subject.as_ref().into())] {
        for predicate_filter in [None, Some(predicate.as_ref())] {
            for object in [None, Some(&plain), Some(&typed), Some(&numeric)] {
                for context in [
                    None,
                    Some(contexts[0].as_ref()),
                    Some(contexts[1].as_ref()),
                    Some(contexts[2].as_ref()),
                ] {
                    // bounded: each stream is a subset of the ten-quad fixture.
                    let actual = view
                        .matching_quads(
                            subject_filter,
                            predicate_filter.map(Into::into),
                            object.map(|value| value.as_ref().into()),
                            context,
                        )
                        .collect::<Result<Vec<_>>>()
                        .unwrap();
                    let logical_object = object.map(|value| {
                        if value.datatype() == oxrdf::vocab::xsd::STRING {
                            typed.as_ref()
                        } else {
                            value.as_ref()
                        }
                    });
                    let reference = expected
                        .matching_quads(
                            subject_filter,
                            predicate_filter.map(Into::into),
                            logical_object.map(Into::into),
                            context,
                        )
                        .collect::<Result<HashSet<_>>>()
                        .unwrap();
                    assert_eq!(actual.len(), reference.len(), "duplicate logical quad");
                    assert_eq!(actual.into_iter().collect::<HashSet<_>>(), reference);
                }
            }
        }
    }
    assert_eq!(view.inner.len().unwrap(), 10);
    for value in [&plain, &typed] {
        assert!(
            !view
                .insert_quad(QuadRef::new(
                    &subject,
                    &predicate,
                    value,
                    GraphNameRef::DefaultGraph,
                ))
                .unwrap()
        );
    }
    assert_eq!(view.inner.len().unwrap(), 10);
    assert!(store.is_empty().unwrap());
}
