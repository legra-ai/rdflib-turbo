use super::*;
use std::panic::{AssertUnwindSafe, catch_unwind};

fn row(value: &str, graph: GraphNameRef<'_>) -> Quad {
    Quad::new(
        NamedNodeRef::new_unchecked("urn:s"),
        NamedNodeRef::new_unchecked("urn:p"),
        Literal::new_simple_literal(value),
        graph,
    )
}

#[test]
fn absent_removal_does_not_promote_additive_journal() {
    let mut data = Dataset::new();
    let mut tx = data.insert_transaction();
    assert!(!tx.remove(&row("missing", GraphNameRef::DefaultGraph)));
    assert!(matches!(tx.journal, MutationJournal::Additive(_)));
    assert!(tx.insert(&row("new", GraphNameRef::DefaultGraph)));
    assert_eq!(tx.inserted().size_hint(), (1, Some(1)));
    let MutationJournal::Additive(entries) = &tx.journal else {
        panic!("unexpected promotion")
    };
    assert_eq!(size_of_val(&entries[0]), size_of::<usize>());
}

#[test]
fn first_state_sets_and_delta_follow_current_presence() {
    let old = row("old", GraphNameRef::DefaultGraph);
    let new = row("new", GraphNameRef::DefaultGraph);
    let gone = row("gone", GraphNameRef::DefaultGraph);
    let mut data = Dataset::from_iter([old.clone()]);
    let mut tx = data.insert_transaction();
    assert!(tx.insert(&new));
    assert!(tx.remove(&new));
    assert!(tx.insert(&new));
    assert!(tx.remove(&old));
    assert!(!tx.remove(&old));
    assert!(tx.insert(&old));
    assert!(tx.insert(&gone));
    assert!(tx.remove(&gone));
    assert_eq!(tx.inserted().collect::<Vec<_>>(), vec![new.as_ref()]);
    let MutationJournal::Mutating { absent, present } = &tx.journal else {
        panic!("not promoted")
    };
    assert_eq!((absent.len(), present.len()), (2, 1));
    assert!(absent.is_disjoint(present));
    tx.commit();
    assert_eq!(data.len(), 2);
    assert!(data.contains(&new));
    assert!(data.contains(&old));
}

#[test]
fn alias_reinsert_rollback_restores_original_arc_in_all_indexes() {
    let original = row("old", GraphNameRef::DefaultGraph);
    let mut data = Dataset::from_iter([original.clone()]);
    let before = data.gspo.first().unwrap().record.clone();
    {
        let mut tx = data.insert_transaction();
        assert!(tx.remove(&original.clone()));
        assert!(tx.insert(&original.clone()));
        assert!(!Arc::ptr_eq(&before, &tx.gspo.first().unwrap().record));
        let MutationJournal::Mutating { present, .. } = &tx.journal else {
            panic!("not promoted")
        };
        assert!(Arc::ptr_eq(&before, present.iter().next().unwrap()));
    }
    for current in [
        &data.gspo.first().unwrap().record,
        &data.gpos.first().unwrap().record,
        &data.gosp.first().unwrap().record,
        &data.spog.first().unwrap().record,
        &data.posg.first().unwrap().record,
        &data.ospg.first().unwrap().record,
    ] {
        assert!(Arc::ptr_eq(&before, current));
    }
}

#[test]
fn mutation_unwind_restores_graph_kinds_before_releasing_new_words() {
    let named = NamedNodeRef::new_unchecked("urn:same");
    let blank = BlankNodeRef::new_unchecked("urn:same");
    let a = row("old", named.into());
    let b = row("old", blank.into());
    let temporary = row("new-word-rollback", GraphNameRef::DefaultGraph);
    let mut data = Dataset::from_iter([a.clone(), b.clone()]);
    let expected = data.clone();
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let mut tx = data.insert_transaction();
            assert!(tx.remove(&a));
            assert!(tx.insert(&temporary));
            assert!(tx.contains(&b));
            panic!("after mutation");
        }))
        .is_err()
    );
    assert_eq!(data, expected);
    assert!(data.encoded_term(temporary.object.as_ref()).is_none());
    assert!(data.contains(&a));
    assert!(data.contains(&b));
}
