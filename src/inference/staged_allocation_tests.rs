//! Boolean membership must not reconstruct owned source terms.
use super::*;
use crate::allocation_tests::measure_allocated;
use oxrdf::GeneralizedDataset;

#[test]
#[ignore = "global allocation counters require isolated execution"]
fn logical_membership_borrows_source_lexical() {
    let quad = GeneralizedQuad::new(
        NamedNode::new_unchecked("urn:s"),
        NamedNode::new_unchecked("urn:p"),
        Literal::new_simple_literal("x".repeat(64 * 1024)),
        NamedNode::new_unchecked("urn:source"),
    );
    let mut source = GeneralizedDataset::new();
    source.insert(&quad);
    let staged = JournalGraph::new(&mut source, GraphName::DefaultGraph);
    let (present, allocated) =
        measure_allocated(|| Graph::contains(&staged, (&quad).into()).unwrap());
    assert!(present);
    println!("STAGED_MEMBERSHIP allocated_bytes={allocated}");
    assert!(
        allocated < 4096,
        "Boolean membership copied source lexical data: {allocated}"
    );
}
