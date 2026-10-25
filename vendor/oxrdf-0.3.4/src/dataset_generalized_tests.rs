use super::*;
use std::hash::{DefaultHasher, Hash, Hasher};

fn rows() -> Vec<GeneralizedQuad> {
    let named = NamedNodeRef::new_unchecked("urn:same");
    let blank = BlankNodeRef::new_unchecked("urn:same");
    let literal = Literal::new_simple_literal("same");
    let mut result = Vec::new();
    for subject in [named.into(), blank.into(), literal.as_ref().into()] {
        for predicate in [named.into(), blank.into(), literal.as_ref().into()] {
            for graph in [GraphNameRef::DefaultGraph, named.into(), blank.into()] {
                result.push(
                    GeneralizedQuadRef {
                        subject,
                        predicate,
                        object: literal.as_ref().into(),
                        graph_name: graph,
                    }
                    .into_owned(),
                );
            }
        }
    }
    #[cfg(feature = "rdf-12")]
    {
        let quoted = Triple::new(named, named, literal.clone());
        result.push(GeneralizedQuad::new(
            quoted.clone(),
            named,
            quoted.clone(),
            named,
        ));
        result.push(GeneralizedQuad::new(
            literal.clone(),
            quoted.clone(),
            quoted,
            blank,
        ));
    }
    result
}

#[test]
fn generalized_storage_preserves_every_role_and_all_pattern_masks() {
    let input = rows();
    let mut data = GeneralizedDataset::new();
    for q in &input {
        assert!(data.insert(q.as_ref()));
        assert!(!data.insert(q.as_ref()));
    }
    assert_eq!(data.len(), input.len());
    assert_eq!(
        data.named_graphs().collect::<HashSet<_>>(),
        HashSet::from([
            NamedOrBlankNodeRef::NamedNode(NamedNodeRef::new_unchecked("urn:same")),
            NamedOrBlankNodeRef::BlankNode(BlankNodeRef::new_unchecked("urn:same")),
        ])
    );
    for q in &input {
        assert!(data.contains(q.as_ref()));
        for mask in 0..16 {
            let q = q.as_ref();
            let s = (mask & 1 != 0).then_some(q.subject);
            let p = (mask & 2 != 0).then_some(q.predicate);
            let o = (mask & 4 != 0).then_some(q.object);
            let g = (mask & 8 != 0).then_some(q.graph_name);
            let expected: HashSet<_> = input
                .iter()
                .filter(|row| {
                    let row = row.as_ref();
                    s.is_none_or(|s| s == row.subject)
                        && p.is_none_or(|p| p == row.predicate)
                        && o.is_none_or(|o| o == row.object)
                        && g.is_none_or(|g| g == row.graph_name)
                })
                .cloned()
                .collect();
            let actual: HashSet<_> = data
                .quads_for_pattern(s, p, o, g)
                .map(GeneralizedQuadRef::into_owned)
                .collect();
            assert_eq!(actual, expected, "mask={mask}");
        }
    }
}

#[test]
fn generalized_record_layout_hash_order_and_six_shared_indexes() {
    let row = Quad::new(
        NamedNodeRef::new_unchecked("urn:moved"),
        NamedNodeRef::new_unchecked("urn:predicate"),
        Literal::new_simple_literal("moved-dictionary-word"),
        NamedNodeRef::new_unchecked("urn:graph"),
    );
    let ordinary = Dataset::from_iter([&row]);
    let original = ordinary.gspo.first().unwrap().record.clone();
    let TermRef::Literal(value) = ordinary.iter().next().unwrap().object else {
        unreachable!()
    };
    let original_word = value.value().as_ptr();
    let moved = GeneralizedDataset::from(ordinary);
    for record in [
        &moved.data.gspo.first().unwrap().record,
        &moved.data.gpos.first().unwrap().record,
        &moved.data.gosp.first().unwrap().record,
        &moved.data.spog.first().unwrap().record,
        &moved.data.posg.first().unwrap().record,
        &moved.data.ospg.first().unwrap().record,
    ] {
        assert!(Arc::ptr_eq(&original, record));
    }
    let TermRef::Literal(value) = moved.iter().next().unwrap().object else {
        unreachable!()
    };
    assert_eq!(value.value().as_ptr(), original_word);
    assert_eq!(moved.iter().collect::<Vec<_>>(), vec![row.as_ref().into()]);
    let general = GeneralizedQuad::from(row.clone());
    for collected in [
        GeneralizedDataset::from_iter([row.clone()]),
        GeneralizedDataset::from_iter([&row]),
        GeneralizedDataset::from_iter([row.as_ref()]),
        GeneralizedDataset::from_iter([general.clone()]),
        GeneralizedDataset::from_iter([&general]),
        GeneralizedDataset::from_iter([general.as_ref()]),
    ] {
        assert_eq!(collected.iter().collect::<Vec<_>>(), vec![general.as_ref()]);
        assert!(collected.contains(&row));
        assert!(collected.contains(&general));
    }
    let mut direct = GeneralizedDataset::new();
    assert!(direct.insert(&row));
    assert!(!direct.insert(&general));
    assert!(direct.remove(&general));
    let mut tx = direct.transaction();
    assert!(tx.insert(&general));
    assert!(!tx.insert(&row));
    assert!(tx.remove(&row));
    tx.commit();
    assert!(direct.is_empty());
    assert_eq!(size_of::<QuadRecord>(), 112);
    let mut data = GeneralizedDataset::new();
    for row in rows() {
        data.insert(row.as_ref());
        let record = data
            .data
            .gspo
            .iter()
            .find(|key| data.decode(&key.record) == row.as_ref())
            .unwrap()
            .record
            .clone();
        let alias = Arc::new((*record).clone());
        assert!(!Arc::ptr_eq(&record, &alias));
        assert_eq!(record, alias);
        assert_eq!(record.cmp(&alias), std::cmp::Ordering::Equal);
        let hash = |r: &QuadRecord| {
            let mut h = DefaultHasher::new();
            r.hash(&mut h);
            h.finish()
        };
        assert_eq!(hash(&record), hash(&alias));
        macro_rules! check {($($index:ident),*) => {$(assert!(Arc::ptr_eq(&record,&data.data.$index.iter().find(|key|key.record==record).unwrap().record));)*};}
        check!(gspo, gpos, gosp, spog, posg, ospg);
    }
    let snapshot = data.clone();
    assert_eq!(
        snapshot.iter().collect::<HashSet<_>>(),
        data.iter().collect()
    );
}

#[test]
fn generalized_journal_restores_original_incarnations_and_current_delta() {
    let input = rows();
    let mut data = GeneralizedDataset::new();
    for row in &input[..4] {
        data.insert(row.as_ref());
    }
    let before: Vec<_> = data.data.gspo.iter().map(|q| q.record.clone()).collect();
    {
        let mut tx = data.transaction();
        assert!(!tx.remove(input[10].as_ref()));
        assert!(matches!(tx.journal, MutationJournal::Additive(_)));
        assert!(tx.insert(input[10].as_ref()));
        assert_eq!(tx.inserted().size_hint(), (1, Some(1)));
        assert!(tx.remove(input[10].as_ref()));
        assert!(tx.insert(input[10].as_ref()));
        assert!(tx.remove(input[0].as_ref()));
        assert!(tx.insert(input[0].as_ref()));
        assert_eq!(tx.inserted().collect::<Vec<_>>(), vec![input[10].as_ref()]);
    }
    assert_eq!(data.len(), 4);
    for record in before {
        assert!(Arc::ptr_eq(
            &record,
            &data.data.gspo.get(record.as_ref()).unwrap().record
        ));
    }
    assert!(!data.contains(input[10].as_ref()));
}

#[test]
fn generalized_unwind_and_commit_preserve_literal_roles_and_dictionary_lifetime() {
    let input = rows();
    let mut data = GeneralizedDataset::new();
    data.insert(input[26].as_ref());
    let before = data.clone();
    let temporary = Literal::new_simple_literal("temporary-new-dictionary-word");
    let q = GeneralizedQuadRef {
        subject: temporary.as_ref().into(),
        predicate: temporary.as_ref().into(),
        object: temporary.as_ref().into(),
        graph_name: GraphNameRef::DefaultGraph,
    };
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut tx = data.transaction();
            assert!(tx.remove(input[26].as_ref()));
            assert!(tx.insert(q));
            panic!("after generalized changes");
        }))
        .is_err()
    );
    assert_eq!(data.iter().collect::<HashSet<_>>(), before.iter().collect());
    assert!(data.data.encoded_term(temporary.as_ref()).is_none());
    let mut tx = data.transaction();
    assert!(tx.remove(input[26].as_ref()));
    assert!(tx.insert(q));
    tx.commit();
    assert_eq!(data.iter().collect::<Vec<_>>(), vec![q]);
}

#[test]
fn six_generalized_orders_and_hashes_match_canonical_term_tuples() {
    let mut data = GeneralizedDataset::new();
    let input = rows();
    for q in &input {
        data.insert(q.as_ref());
    }
    let tuples: Vec<_> = input
        .iter()
        .map(|q| {
            let i = &data.data.interner;
            (
                InternedTerm::encoded_from(q.subject.as_ref(), i).unwrap(),
                InternedTerm::encoded_from(q.predicate.as_ref(), i).unwrap(),
                InternedTerm::encoded_from(q.object.as_ref(), i).unwrap(),
                InternedGraphName::encoded_from(q.graph_name.as_ref(), i).unwrap(),
            )
        })
        .collect();
    for a in &tuples {
        let record = QuadRecord::from_generalized(a.clone());
        let mut expected_hash = DefaultHasher::new();
        a.hash(&mut expected_hash);
        let mut actual_hash = DefaultHasher::new();
        record.hash(&mut actual_hash);
        assert_eq!(actual_hash.finish(), expected_hash.finish());
        for b in &tuples {
            let other = QuadRecord::from_generalized(b.clone());
            assert_eq!(record == other, a == b);
            macro_rules! order {
                ($key:ident,($i:tt,$j:tt,$k:tt,$l:tt)) => {{
                    let actual = $key::new(Arc::new(record.clone()))
                        .cmp(&$key::new(Arc::new(other.clone())));
                    let expected = (&a.$i, &a.$j, &a.$k, &a.$l).cmp(&(&b.$i, &b.$j, &b.$k, &b.$l));
                    assert_eq!(actual, expected);
                }};
            }
            order!(GspoKey, (3, 0, 1, 2));
            order!(GposKey, (3, 1, 2, 0));
            order!(GospKey, (3, 2, 0, 1));
            order!(SpogKey, (0, 1, 2, 3));
            order!(PosgKey, (1, 2, 0, 3));
            order!(OspgKey, (2, 0, 1, 3));
        }
    }
}

#[test]
fn every_small_mutation_sequence_keeps_first_state_delta_and_undo() {
    // bounded: three mixed-role facts, eight initial states and all216 length3
    // insert/remove sequences; no relation-sized test enumeration.
    let input = rows();
    let facts = [input[0].clone(), input[10].clone(), input[26].clone()];
    for initial in 0..8 {
        for sequence in 0..216 {
            let mut data = GeneralizedDataset::new();
            let mut expected = HashSet::new();
            for (i, q) in facts.iter().enumerate() {
                if initial & (1 << i) != 0 {
                    data.insert(q.as_ref());
                    expected.insert(q.clone());
                }
            }
            let before = expected.clone();
            let original: Vec<_> = data
                .data
                .gspo
                .iter()
                .map(|key| key.record.clone())
                .collect();
            let mut tx = data.transaction();
            let mut code = sequence;
            for _ in 0..3 {
                let operation = code % 6;
                code /= 6;
                let fact = &facts[operation / 2];
                if operation % 2 == 0 {
                    assert_eq!(tx.insert(fact.as_ref()), expected.insert(fact.clone()));
                } else {
                    assert_eq!(tx.remove(fact.as_ref()), expected.remove(fact));
                }
                assert_eq!(
                    tx.iter()
                        .map(GeneralizedQuadRef::into_owned)
                        .collect::<HashSet<_>>(),
                    expected
                );
                assert_eq!(
                    tx.inserted()
                        .map(GeneralizedQuadRef::into_owned)
                        .collect::<HashSet<_>>(),
                    expected.difference(&before).cloned().collect()
                );
                if let MutationJournal::Mutating { absent, present } = &tx.journal {
                    assert!(absent.is_disjoint(present));
                }
                let lengths = [
                    (*tx).data.gspo.len(),
                    (*tx).data.gpos.len(),
                    (*tx).data.gosp.len(),
                    (*tx).data.spog.len(),
                    (*tx).data.posg.len(),
                    (*tx).data.ospg.len(),
                ];
                assert_eq!(lengths, [expected.len(); 6]);
            }
            if sequence % 2 == 0 {
                drop(tx);
                assert_eq!(
                    data.iter()
                        .map(GeneralizedQuadRef::into_owned)
                        .collect::<HashSet<_>>(),
                    before
                );
                for record in original {
                    assert!(Arc::ptr_eq(
                        &record,
                        &data.data.gspo.get(record.as_ref()).unwrap().record
                    ));
                }
            } else {
                tx.commit();
                assert_eq!(
                    data.iter()
                        .map(GeneralizedQuadRef::into_owned)
                        .collect::<HashSet<_>>(),
                    expected
                );
            }
        }
    }
}

#[test]
fn generalized_equality_compares_full_roles_and_graph_kinds_independent_of_insert_order() {
    let input = rows();
    let left = GeneralizedDataset::from_iter(input.iter());
    let mut right = GeneralizedDataset::from_iter(input.iter().rev());
    assert_eq!(left, right);
    right.remove(&input[0]);
    assert_ne!(left, right);
    right.insert(&input[0]);
    assert_eq!(left, right);
}
