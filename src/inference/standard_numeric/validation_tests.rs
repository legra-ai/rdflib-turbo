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
