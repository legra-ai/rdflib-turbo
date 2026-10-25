use super::*;
use std::cell::Cell;
thread_local! {static CANDIDATE_ROWS: Cell<usize> = const { Cell::new(0) };}
pub(super) fn candidate_row_visited() {
    CANDIDATE_ROWS.with(|rows| rows.set(rows.get() + 1));
}
#[test]
fn scoped_patterns_do_not_decode_unrelated_graph_rows() {
    let subject = NamedNode::new_unchecked("urn:s");
    let predicate = NamedNode::new_unchecked("urn:p");
    let object = Literal::new_simple_literal("value");
    let mut data = Dataset::new();
    data.insert(QuadRef::new(
        &subject,
        &predicate,
        &object,
        GraphNameRef::DefaultGraph,
    ));
    for id in 0..1000 {
        data.insert(QuadRef::new(
            &subject,
            &predicate,
            &object,
            &NamedNode::new_unchecked(format!("urn:g{id}")),
        ));
    }
    for graph in [
        GraphName::DefaultGraph,
        NamedNode::new_unchecked("urn:g42").into(),
    ] {
        for mask in [1, 2, 4, 7] {
            CANDIDATE_ROWS.with(|rows| rows.set(0));
            assert_eq!(
                data.quads_for_pattern(
                    (mask & 1 != 0).then_some(subject.as_ref().into()),
                    (mask & 2 != 0).then_some(predicate.as_ref()),
                    (mask & 4 != 0).then_some(object.as_ref().into()),
                    Some(graph.as_ref())
                )
                .count(),
                1
            );
            let visited = CANDIDATE_ROWS.with(Cell::get);
            println!("SCOPED_VISITS graph={graph} mask={mask} rows={visited}");
            assert_eq!(
                visited, 1,
                "scoped mask{mask} visited unrelated graph rows or missed the actual range/point boundary"
            );
        }
    }
}

#[test]
fn iterator_retains_dataset_terms_after_temporary_patterns_drop() {
    let mut data = Dataset::new();
    data.insert(&Quad::new(
        NamedNode::new_unchecked("urn:s"),
        NamedNode::new_unchecked("urn:p"),
        Literal::new_simple_literal("value"),
        NamedNode::new_unchecked("urn:g"),
    ));
    let rows = {
        let s = NamedNode::new_unchecked("urn:s");
        let p = NamedNode::new_unchecked("urn:p");
        let o = Literal::new_simple_literal("value");
        let g = NamedNode::new_unchecked("urn:g");
        data.quads_for_pattern(
            Some(s.as_ref().into()),
            Some(p.as_ref()),
            Some(o.as_ref().into()),
            Some(g.as_ref().into()),
        )
    };
    assert_eq!(rows.count(), 1);
}

#[test]
fn scoped_result_order_matches_existing_global_index_filtering() {
    // bounded: the fixed exact-term fixture, not caller-sized application input.
    let data = Dataset::from_iter(shared_tests::fixture());
    let targets: Vec<_> = data.iter().step_by(37).map(QuadRef::into_owned).collect();
    for target in &targets {
        for mask in 0..8 {
            let subject = (mask & 1 != 0).then_some(target.subject.as_ref());
            let predicate = (mask & 2 != 0).then_some(target.predicate.as_ref());
            let object = (mask & 4 != 0).then_some(target.object.as_ref());
            let original: Box<dyn Iterator<Item = QuadRef<'_>>> = if let Some(s) = subject {
                Box::new(data.quads_for_subject(s))
            } else if let Some(o) = object {
                Box::new(data.quads_for_object(o))
            } else if let Some(p) = predicate {
                Box::new(data.quads_for_predicate(p))
            } else {
                Box::new(data.quads_for_graph_name(target.graph_name.as_ref()))
            };
            let expected: Vec<_> = original
                .filter(|q| {
                    subject.is_none_or(|s| s == q.subject)
                        && predicate.is_none_or(|p| p == q.predicate)
                        && object.is_none_or(|o| o == q.object)
                        && q.graph_name == target.graph_name.as_ref()
                })
                .map(QuadRef::into_owned)
                .collect();
            let actual: Vec<_> = data
                .quads_for_pattern(subject, predicate, object, Some(target.graph_name.as_ref()))
                .map(QuadRef::into_owned)
                .collect();
            assert_eq!(actual, expected, "observed order differs for mask{mask}");
        }
    }
}

// The counters live at actual Dataset encoding boundaries that hash lexical inputs.
// They are thread-local test instrumentation, not Dataset fields or production state.
thread_local! { static ENCODE_CALLS: Cell<[usize; 4]> = const { Cell::new([0; 4]) }; }
pub(super) fn term_encoded(component: usize) {
    ENCODE_CALLS.with(|counts| {
        let mut current = counts.get();
        current[component] += 1;
        counts.set(current);
    });
}

#[test]
fn pattern_encodes_each_supplied_component_once_for_every_mask() {
    let subject = NamedNode::new_unchecked(format!("urn:subject:{}", "s".repeat(4096)));
    let predicate = NamedNode::new_unchecked(format!("urn:predicate:{}", "p".repeat(4096)));
    let object = Literal::new_simple_literal("object".repeat(4096));
    let graph = NamedNode::new_unchecked(format!("urn:graph:{}", "g".repeat(4096)));
    let mut data = Dataset::new();
    for context in [GraphName::DefaultGraph, graph.clone().into()] {
        data.insert(&Quad::new(
            subject.clone(),
            predicate.clone(),
            object.clone(),
            context,
        ));
    }
    for mask in 0..16 {
        ENCODE_CALLS.with(|counts| counts.set([0; 4]));
        let rows = data
            .quads_for_pattern(
                (mask & 1 != 0).then_some(subject.as_ref().into()),
                (mask & 2 != 0).then_some(predicate.as_ref()),
                (mask & 4 != 0).then_some(object.as_ref().into()),
                (mask & 8 != 0).then_some(graph.as_ref().into()),
            )
            .count();
        let expected_rows = if mask & 8 != 0 { 1 } else { 2 };
        assert_eq!(rows, expected_rows, "mask{mask} result changed");
        let calls = ENCODE_CALLS.with(Cell::get);
        let expected = [
            usize::from(mask & 1 != 0),
            usize::from(mask & 2 != 0),
            usize::from(mask & 4 != 0),
            usize::from(mask & 8 != 0),
        ];
        println!("PATTERN_ENCODINGS mask={mask} calls={calls:?}");
        assert_eq!(
            calls, expected,
            "mask{mask} hashed supplied lexical input repeatedly"
        );
    }
    let missing = Literal::new_simple_literal("not in the Dataset");
    ENCODE_CALLS.with(|counts| counts.set([0; 4]));
    assert_eq!(
        data.quads_for_pattern(
            Some(subject.as_ref().into()),
            Some(predicate.as_ref()),
            Some(missing.as_ref().into()),
            Some(graph.as_ref().into())
        )
        .count(),
        0
    );
    assert!(
        ENCODE_CALLS
            .with(Cell::get)
            .into_iter()
            .all(|calls| calls <= 1)
    );
}
