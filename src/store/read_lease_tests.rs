//! Native lexical owners remain valid after their store and cursor are gone.
use super::{GraphName, LexicalTerm, Literal, NamedNode, Store, insert, open, start};

#[test]
fn lexical_rows_share_dictionary_payloads_and_outlive_the_store() {
    let store = Store::new().unwrap();
    // bounded: four fixed native literal forms, each inserted twice.
    let cases = [
        Literal::new_simple_literal("x".repeat(4096)),
        Literal::new_typed_literal(
            "explicit string",
            NamedNode::new_unchecked("http://www.w3.org/2001/XMLSchema#string"),
        ),
        Literal::new_language_tagged_literal("bonjour", "fr").unwrap(),
        Literal::new_typed_literal(
            "01",
            NamedNode::new_unchecked("http://www.w3.org/2001/XMLSchema#integer"),
        ),
    ];
    for (index, literal) in cases.iter().enumerate() {
        for copy in 0..2 {
            insert(
                &store,
                &format!("urn:s{index}:{copy}"),
                literal.clone(),
                GraphName::DefaultGraph,
            );
        }
    }
    let (rows, cursor) = start(&store, None, None, None, None, 7).unwrap();
    assert!(cursor.is_none());
    drop(store);
    for expected in cases {
        let mut matches = rows.iter().filter_map(|(_, _, object)| match object {
            Some(LexicalTerm::Literal {
                value,
                datatype,
                language,
            }) if value.as_str() == expected.value() => Some((value, datatype, language)),
            _ => None,
        });
        let (first, first_datatype, first_language) = matches.next().unwrap();
        let (second, second_datatype, second_language) = matches.next().unwrap();
        assert!(matches.next().is_none());
        assert_eq!(first.as_str(), expected.value());
        assert_eq!(
            first_language.as_ref().map(|text| text.as_str()),
            expected.language()
        );
        assert_eq!(
            second_language.as_ref().map(|text| text.as_str()),
            expected.language()
        );
        let datatype = (expected.language().is_none() && expected.has_explicit_datatype())
            .then(|| expected.datatype());
        assert_eq!(
            first_datatype.as_ref().map(|text| text.as_str()),
            datatype.map(|node| node.as_str())
        );
        if first.as_str().len() > 15 {
            assert_eq!(first.as_str().as_ptr(), second.as_str().as_ptr());
        }
        if let (Some(first), Some(second)) = (first_datatype, second_datatype) {
            assert_eq!(first.as_str().as_ptr(), second.as_str().as_ptr());
        }
    }
    let mut predicates = rows.iter().map(|(_, predicate, _)| match predicate {
        Some(LexicalTerm::NamedNode(value)) => value.as_str(),
        _ => panic!("IRI predicate expected"),
    });
    let first = predicates.clone().next().unwrap();
    assert!(predicates.all(|value| value.as_ptr() == first.as_ptr()));
}

#[cfg(feature = "allocation-profiling")]
#[test]
#[ignore = "global allocation counters require this test to run alone with --test-threads=1"]
fn scoped_lexical_scan_does_not_copy_stored_payloads() {
    let store = Store::new().unwrap();
    for index in 0..1000 {
        insert(
            &store,
            &format!("urn:s{index}"),
            Literal::new_simple_literal("x".repeat(4096)),
            GraphName::DefaultGraph,
        );
    }
    let (count, bytes) = crate::allocation_tests::measure_allocated(|| {
        let cursor = open(&store, None, None, None, None, 4).unwrap();
        let mut count = 0;
        loop {
            let (rows, finished) = cursor.batch().unwrap();
            count += rows.len();
            std::hint::black_box(&rows);
            if finished {
                return count;
            }
        }
    });
    assert_eq!(count, 1000);
    assert!(
        bytes < 512 * 1024,
        "Scoped lexical scan copied unused native payloads: {bytes} bytes requested"
    );
}

#[test]
fn short_rows_after_a_byte_limited_batch_keep_capacity_bounded() {
    let store = Store::new().unwrap();
    for index in 0..256 {
        insert(
            &store,
            &format!("urn:s{index}"),
            Literal::new_simple_literal("short"),
            GraphName::DefaultGraph,
        );
    }
    let cursor = open(&store, None, None, None, None, 4).unwrap();
    // A byte-limited preceding batch can leave any positive row-count hint.
    cursor.state.lock().unwrap().capacity_hint = 19;
    let (rows, finished) = cursor.batch().unwrap();
    assert!(!finished);
    assert_eq!(rows.len(), super::super::BATCH_ROWS);
    assert!(
        rows.capacity() <= super::super::BATCH_ROWS,
        "{} slots",
        rows.capacity()
    );
}
