//! Storage failures must not publish partial round results or retain journal writes.
#[path = "failure_storage_tests.rs"]
mod storage;

use oxigraph::model::Dataset;

use super::{Pending, intersections, same_as};
use crate::Error;
use storage::{Activation, FAILURE, Fault, Scan, Storage, quad};

enum Kernel {
    Equality,
    Intersection,
}

fn failure_rolls_back(kernel: Kernel, input: &[(&str, &str, &str)], fault: Fault, partial: bool) {
    // bounded: these hand-written synthetic fixtures contain at most a few triples.
    let mut data = Dataset::from_iter(input.iter().map(|(s, p, o)| quad(s, p, o)));
    let original = data.clone();
    let previous_round = quad("previous-round", "p", "staged");
    {
        let mut journal = data.insert_transaction();
        journal.insert(previous_round.as_ref());
        let source = Storage::new(&journal, fault);
        let mut pending = Pending::default();
        let error = match kernel {
            Kernel::Equality => same_as::apply(&source, &mut pending),
            Kernel::Intersection => intersections::additions(&source, &mut pending),
        }
        .expect_err("storage error was swallowed or retried");
        assert!(
            source.triggered(),
            "fixture did not reach its selected storage boundary"
        );
        assert!(matches!(error, Error::Runtime(ref text) if text == FAILURE));
        if partial {
            assert!(
                !pending.0.is_empty(),
                "fixture must fail after producing some pending output"
            );
        }
        for quad in &pending.0 {
            assert!(
                !journal.contains(quad),
                "pending result became visible before round completion"
            );
        }
        assert!(journal.contains(&previous_round));
        // Return with an error: the real insertion journal is dropped without commit.
    }
    assert_eq!(
        data, original,
        "failed inference retained a prior round's staged writes"
    );
    let recovery = quad("recovered", "p", "value");
    let mut journal = data.insert_transaction();
    journal.insert(recovery.as_ref());
    journal.commit();
    assert!(
        data.contains(&recovery),
        "rollback prevented a subsequent real commit"
    );
}

#[test]
fn equality_iterator_and_row_failures_abort_without_publication() {
    let input = [("a", "sameAs", "b"), ("b", "sameAs", "c")];
    for (scan, activation, after_results, partial) in [
        (
            Scan::new(None, Some("sameAs"), None),
            Activation::Immediate,
            1,
            true,
        ),
        (
            Scan::new(Some("a"), Some("sameAs"), None),
            Activation::Immediate,
            0,
            true,
        ),
        (
            Scan::new(Some("a"), Some("sameAs"), None),
            Activation::AfterEqualityEnumeration,
            0,
            true,
        ),
        (
            Scan::new(Some("b"), Some("sameAs"), None),
            Activation::AfterEqualityEnumeration,
            0,
            true,
        ),
        (
            Scan::new(None, None, None),
            Activation::AfterEqualityEnumeration,
            1,
            true,
        ),
    ] {
        failure_rolls_back(
            Kernel::Equality,
            &input,
            Fault {
                scan,
                activation,
                after_results,
            },
            partial,
        );
    }
}

#[test]
fn equality_symmetry_coneighbor_and_transitive_membership_failures_abort() {
    for (scan, activation) in [
        (
            Scan::new(Some("b"), Some("sameAs"), Some("a")),
            Activation::Immediate,
        ),
        (
            Scan::new(Some("b"), Some("sameAs"), Some("b")),
            Activation::AfterEqualityEnumeration,
        ),
        (
            Scan::new(Some("a"), Some("sameAs"), Some("c")),
            Activation::AfterEqualityEnumeration,
        ),
    ] {
        failure_rolls_back(
            Kernel::Equality,
            &[("a", "sameAs", "b"), ("b", "sameAs", "c")],
            Fault {
                scan,
                activation,
                after_results: 0,
            },
            false,
        );
    }
}

#[test]
fn replacement_subject_and_object_storage_failures_abort() {
    for (input, scan) in [
        (
            vec![("a", "sameAs", "b"), ("a", "p", "value")],
            Scan::new(Some("a"), Some("sameAs"), None),
        ),
        (
            vec![("a", "sameAs", "b"), ("a", "p", "value")],
            Scan::new(Some("b"), Some("p"), Some("value")),
        ),
        (
            vec![("b", "sameAs", "c"), ("a", "p", "b")],
            Scan::new(Some("b"), Some("sameAs"), None),
        ),
        (
            vec![("b", "sameAs", "c"), ("a", "p", "b")],
            Scan::new(Some("a"), Some("p"), Some("c")),
        ),
    ] {
        failure_rolls_back(
            Kernel::Equality,
            &input,
            Fault {
                scan,
                activation: Activation::AfterUnfilteredReadStarts,
                after_results: 0,
            },
            false,
        );
    }
}

#[test]
fn intersection_reachability_candidate_and_membership_failures_abort() {
    let input = [
        ("C", "intersectionOf", "list"),
        ("list", "first", "Member"),
        ("list", "rest", "nil"),
        ("x", "type", "Member"),
        ("y", "type", "Member"),
    ];
    for (scan, after_results, partial) in [
        (Scan::new(None, Some("intersectionOf"), None), 0, false),
        (Scan::new(Some("list"), Some("first"), None), 0, false),
        (Scan::new(Some("list"), Some("rest"), None), 0, false),
        (Scan::new(None, Some("type"), Some("Member")), 0, false),
        (Scan::new(None, Some("type"), Some("Member")), 1, true),
        (Scan::new(Some("x"), Some("type"), Some("Member")), 0, false),
        (Scan::new(Some("x"), Some("type"), Some("C")), 0, false),
    ] {
        failure_rolls_back(
            Kernel::Intersection,
            &input,
            Fault {
                scan,
                activation: Activation::Immediate,
                after_results,
            },
            partial,
        );
    }
}

#[test]
fn dense_missing_pair_probe_failures_abort_without_publication() {
    let input = [
        ("a", "sameAs", "b"),
        ("a", "sameAs", "c"),
        ("b", "sameAs", "b"),
        ("b", "sameAs", "c"),
    ];
    for (scan, after_results) in [
        (Scan::new(Some("b"), Some("sameAs"), Some("a")), 0),
        (Scan::new(None, Some("sameAs"), Some("b")), 1),
    ] {
        failure_rolls_back(
            Kernel::Equality,
            &input,
            Fault {
                scan,
                activation: Activation::AfterEqualityEnumeration,
                after_results,
            },
            true,
        );
    }
}
