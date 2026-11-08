//! Reject unsupported adapter syntax before any source-dependent join.
use super::*;
use oxrdf::GeneralizedDataset;

#[test]
fn unsupported_operator_cannot_hide_behind_an_empty_relation() {
    let mut rule = OwlNumericRuleId::DtEq.definition();
    let Body::Positive(atom) = &rule.body[0] else {
        panic!("fixed catalogue shape");
    };
    rule.body.push(Body::Negative(atom.clone()));
    assert!(rule.is_safe());
    let program = Program { rules: vec![rule] };
    let mut empty = GeneralizedDataset::new();
    let transaction = empty.transaction();
    let mut pending = Pending::default();
    let mut diagnostics = Diagnostics::new(Vec::new(), 0);
    assert!(
        program
            .apply(&transaction, &mut pending, &mut diagnostics)
            .is_err()
    );
}

#[test]
fn repeated_literal_occurrences_have_one_streamed_domain_witness() {
    let mut data = GeneralizedDataset::new();
    let literal = Literal::new_typed_literal(
        "1",
        NamedNode::new_unchecked("http://www.w3.org/2001/XMLSchema#integer"),
    );
    for subject in ["urn:a", "urn:b", "urn:c"] {
        data.insert(
            GeneralizedQuad::new(
                NamedNode::new_unchecked(subject),
                NamedNode::new_unchecked("urn:p"),
                literal.clone(),
                GraphName::DefaultGraph,
            )
            .as_ref(),
        );
    }
    data.insert(
        GeneralizedQuad::new(
            literal.clone(),
            NamedNode::new_unchecked("urn:p"),
            literal,
            GraphName::DefaultGraph,
        )
        .as_ref(),
    );
    let transaction = data.transaction();
    let source = Source {
        graph: &transaction,
    };
    let mut witnesses = 0;
    source
        .literals(None, |_, _| {
            witnesses += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(witnesses, 1);
}
