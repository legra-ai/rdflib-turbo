#[cfg(feature = "rdf-12")]
use oxigraph::model::{BaseDirection, Triple};
use oxigraph::model::{BlankNode, GraphName, GraphNameRef, Literal, NamedNode, Quad, Term};
use oxigraph::store::{LexicalTerm, Store};

#[test]
fn dictionary_payload_is_shared_across_independent_reads_and_survives_store_drop() {
    let store = Store::new().unwrap();
    // bounded: one 4 MiB payload proving reads lease dictionary text instead of cloning it.
    let lexical = "x".repeat(4 * 1024 * 1024);
    let node = || NamedNode::new_unchecked("urn:shared:long-identifier-for-dictionary");
    store
        .insert(&Quad::new(
            node(),
            node(),
            Literal::new_simple_literal(&lexical),
            GraphName::DefaultGraph,
        ))
        .unwrap();
    let first = store
        .iter()
        .project_lexical(true, false, true)
        .next()
        .unwrap()
        .unwrap();
    let second = store
        .iter()
        .project_lexical(true, false, true)
        .next()
        .unwrap()
        .unwrap();
    let Some(LexicalTerm::NamedNode(a)) = &first[0] else {
        panic!("expected named node")
    };
    let Some(LexicalTerm::NamedNode(b)) = &second[0] else {
        panic!("expected named node")
    };
    assert!(
        std::ptr::eq(a.as_str().as_ptr(), b.as_str().as_ptr()),
        "identifier read cloned dictionary payload"
    );
    let Some(LexicalTerm::Literal { value: a, .. }) = &first[2] else {
        panic!("expected literal")
    };
    let Some(LexicalTerm::Literal { value: b, .. }) = &second[2] else {
        panic!("expected literal")
    };
    assert!(
        std::ptr::eq(a.as_str().as_ptr(), b.as_str().as_ptr()),
        "literal read cloned dictionary payload"
    );
    drop(store);
    assert_eq!(a.as_str(), lexical);
    assert_eq!(b.as_str(), lexical);
}

fn restore(term: LexicalTerm) -> oxigraph::model::Term {
    match term {
        LexicalTerm::NamedNode(value) => NamedNode::new_unchecked(value.as_str()).into(),
        LexicalTerm::BlankNode(value) => BlankNode::new_unchecked(value.as_str()).into(),
        LexicalTerm::Literal {
            value,
            datatype,
            language,
            #[cfg(feature = "rdf-12")]
            direction,
        } => {
            #[cfg(feature = "rdf-12")]
            if let Some(direction) = direction {
                return Literal::new_directional_language_tagged_literal_unchecked(
                    value.as_str(),
                    language.unwrap().as_str(),
                    direction,
                )
                .into();
            }
            if let Some(language) = language {
                Literal::new_language_tagged_literal_unchecked(value.as_str(), language.as_str())
                    .into()
            } else if let Some(datatype) = datatype {
                Literal::new_typed_literal(
                    value.as_str(),
                    NamedNode::new_unchecked(datatype.as_str()),
                )
                .into()
            } else {
                Literal::new_simple_literal(value.as_str()).into()
            }
        }
        #[cfg(feature = "rdf-12")]
        LexicalTerm::Triple(triple) => {
            let [s, p, o] = *triple;
            let Term::NamedNode(p) = restore(p) else {
                panic!("expected predicate")
            };
            oxigraph::model::Triple::new(
                oxigraph::model::NamedOrBlankNode::try_from(restore(s)).unwrap(),
                p,
                restore(o),
            )
            .into()
        }
    }
}

fn fixture(store: &Store) {
    let node = || NamedNode::new_unchecked("urn:predicate");
    for (index, value) in ["x".to_owned(), "x".repeat(256)].into_iter().enumerate() {
        for (suffix, literal) in [
            (0, Literal::new_simple_literal(&value)),
            (
                1,
                Literal::new_language_tagged_literal_unchecked(&value, "en"),
            ),
            (
                2,
                Literal::new_language_tagged_literal_unchecked(&value, "en-abcdefgh-abcdefgh"),
            ),
            (
                3,
                Literal::new_typed_literal(
                    &value,
                    NamedNode::new_unchecked("urn:datatype:custom-long"),
                ),
            ),
            (
                4,
                Literal::new_typed_literal(
                    &value,
                    NamedNode::new_unchecked("http://www.w3.org/2001/XMLSchema#string"),
                ),
            ),
        ] {
            store
                .insert(&Quad::new(
                    NamedNode::new_unchecked(format!("urn:s{index}:{suffix}")),
                    node(),
                    literal,
                    GraphName::DefaultGraph,
                ))
                .unwrap();
        }
    }
    for blank in [
        BlankNode::new_from_unique_id(123),
        BlankNode::new_unchecked("small"),
        BlankNode::new_unchecked("a-long-blank-node-identifier"),
    ] {
        store
            .insert(&Quad::new(
                blank.clone(),
                node(),
                blank,
                NamedNode::new_unchecked("urn:named"),
            ))
            .unwrap();
    }
    for (index, (datatype, value)) in [
        ("boolean", "true"),
        ("float", "1.0"),
        ("double", "1.0"),
        ("integer", "01"),
        ("decimal", "+01.00"),
        ("dateTime", "2001-02-03T04:05:06Z"),
        ("time", "04:05:06Z"),
        ("date", "2001-02-03Z"),
        ("gYearMonth", "2001-02Z"),
        ("gYear", "2001Z"),
        ("gMonthDay", "--02-03Z"),
        ("gDay", "---03Z"),
        ("gMonth", "--02Z"),
        ("duration", "P1D"),
        ("yearMonthDuration", "P1Y"),
        ("dayTimeDuration", "P1D"),
    ]
    .into_iter()
    .enumerate()
    {
        store
            .insert(&Quad::new(
                NamedNode::new_unchecked(format!("urn:numeric:{index}")),
                node(),
                Literal::new_typed_literal(
                    value,
                    NamedNode::new_unchecked(format!(
                        "http://www.w3.org/2001/XMLSchema#{datatype}"
                    )),
                ),
                GraphName::DefaultGraph,
            ))
            .unwrap();
    }
}

fn compare_all_masks(store: &Store) {
    for graph in [
        GraphNameRef::DefaultGraph,
        NamedNode::new_unchecked("urn:named").as_ref().into(),
    ] {
        for mask in 0..8 {
            let selected = [mask & 1 != 0, mask & 2 != 0, mask & 4 != 0];
            // bounded: this contract has fewer than 40 fixed fixture statements.
            let expected: Vec<_> = store
                .quads_for_pattern(None, None, None, Some(graph))
                .project(selected[0], selected[1], selected[2])
                .map(Result::unwrap)
                .collect();
            let actual: Vec<[Option<Term>; 3]> = store
                .quads_for_pattern(None, None, None, Some(graph))
                .project_lexical(selected[0], selected[1], selected[2])
                .map(|row| row.unwrap().map(|term| term.map(restore)))
                .collect();
            assert_eq!(actual, expected, "mask {mask}, graph {graph}");
        }
    }
}

#[test]
fn lexical_projection_matches_existing_decode_for_all_masks_and_encodings() {
    let store = Store::new().unwrap();
    fixture(&store);
    compare_all_masks(&store);
}

#[test]
fn lexical_cursor_owns_snapshot_and_preserves_transaction_borrowing() {
    let store = Store::new().unwrap();
    let node = || NamedNode::new_unchecked("urn:before");
    let quad = Quad::new(
        node(),
        node(),
        Literal::new_simple_literal("before"),
        GraphName::DefaultGraph,
    );
    store.insert(&quad).unwrap();
    let mut snapshot = store.iter().project_lexical(true, false, true);
    store.remove(&quad).unwrap();
    drop(store);
    let row = snapshot.next().unwrap().unwrap();
    assert_eq!(restore(row[0].clone().unwrap()), node().into());
    assert!(snapshot.next().is_none());
    let store = Store::new().unwrap();
    let mut transaction = store.start_transaction().unwrap();
    transaction.insert(&quad);
    let mut cursor = transaction.iter().project_lexical(false, false, true);
    assert_eq!(
        restore(cursor.next().unwrap().unwrap()[2].clone().unwrap()),
        quad.object
    );
    drop(cursor);
    drop(transaction);
    assert!(store.is_empty().unwrap());
}

#[cfg(feature = "rdf-12")]
#[test]
fn lexical_projection_keeps_rdf12_direction_and_nested_triple_terms() {
    let store = Store::new().unwrap();
    let node = || NamedNode::new_unchecked("urn:node");
    for value in ["x".to_owned(), "x".repeat(256)] {
        for language in ["en", "en-abcdefgh-abcdefgh"] {
            for direction in [BaseDirection::Ltr, BaseDirection::Rtl] {
                store
                    .insert(&Quad::new(
                        node(),
                        node(),
                        Literal::new_directional_language_tagged_literal_unchecked(
                            &value, language, direction,
                        ),
                        GraphName::DefaultGraph,
                    ))
                    .unwrap();
            }
        }
    }
    store
        .insert(&Quad::new(
            node(),
            node(),
            Triple::new(
                node(),
                node(),
                Triple::new(node(), node(), Literal::new_simple_literal("nested")),
            ),
            GraphName::DefaultGraph,
        ))
        .unwrap();
    compare_all_masks(&store);
}

#[cfg(feature = "persistence")]
#[test]
fn persistent_lexical_owners_survive_reader_and_store_drop() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    fixture(&store);
    compare_all_masks(&store);
    let row = store
        .iter()
        .project_lexical(true, true, true)
        .next()
        .unwrap()
        .unwrap();
    let expected = row.clone().map(|term| term.map(restore));
    drop(store);
    assert_eq!(row.map(|term| term.map(restore)), expected);
    let reopened = Store::open(directory.path()).unwrap();
    compare_all_masks(&reopened);
}
