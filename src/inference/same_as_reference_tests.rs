//! Independent full-Term algebra over a single immutable synthetic test snapshot.
use std::collections::HashSet;

use oxigraph::model::*;

use super::{Graph, OWL};
use crate::Result;

pub(super) fn one_round(data: &impl Graph) -> Result<HashSet<GeneralizedQuad>> {
    // bounded: callers use fixed small synthetic fixtures; never production data.
    let input = data.quads(None, None, None).collect::<Result<Vec<_>>>()?;
    let same: Term = NamedNode::new(format!("{OWL}sameAs")).unwrap().into();
    let mut output = HashSet::new();
    let mut emit = |subject: &Term, predicate: &Term, object: &Term| -> Result<()> {
        let quad = GeneralizedQuad::new(
            subject.clone(),
            predicate.clone(),
            object.clone(),
            GraphName::DefaultGraph,
        );
        if !predicate.is_literal() && !data.contains(quad.as_ref())? {
            output.insert(quad);
        }
        Ok(())
    };
    for equality in input.iter().filter(|row| row.predicate == same) {
        emit(&equality.object, &same, &equality.subject)?;
        for row in &input {
            if row.predicate == same && row.subject == equality.object {
                emit(&equality.subject, &same, &row.object)?;
            }
            if row.subject == equality.subject {
                emit(&equality.object, &row.predicate, &row.object)?;
            }
            if row.object == equality.subject {
                emit(&row.subject, &row.predicate, &equality.object)?;
            }
        }
    }
    Ok(output)
}

/// All six original equality clauses, separate from the four-rule indexed kernel.
pub(super) fn equality_rules_one_round(data: &impl Graph) -> Result<HashSet<GeneralizedQuad>> {
    let mut output = one_round(data)?;
    // bounded: finite tiny exhaustive/original-reference fixtures only.
    let input = data.quads(None, None, None).collect::<Result<Vec<_>>>()?;
    let same: Term = NamedNode::new(format!("{OWL}sameAs")).unwrap().into();
    let mut emit = |subject: &Term, predicate: &Term, object: &Term| -> Result<()> {
        let quad = GeneralizedQuad::new(
            subject.clone(),
            predicate.clone(),
            object.clone(),
            GraphName::DefaultGraph,
        );
        if !predicate.is_literal() && !data.contains(quad.as_ref())? {
            output.insert(quad);
        }
        Ok(())
    };
    for row in &input {
        for term in [&row.subject, &row.predicate, &row.object] {
            emit(term, &same, term)?;
        }
    }
    for equality in input.iter().filter(|row| row.predicate == same) {
        for row in input.iter().filter(|row| row.predicate == equality.subject) {
            emit(&row.subject, &equality.object, &row.object)?;
        }
    }
    Ok(output)
}

#[derive(serde::Deserialize)]
struct OriginalRound {
    input: Vec<[[String; 2]; 3]>,
    expected: Vec<[[String; 2]; 3]>,
}

fn original_row(row: [[String; 2]; 3]) -> GeneralizedQuad {
    let [subject, predicate, object]: [Term; 3] = row.map(|[kind, value]| match kind.as_str() {
        "iri" => NamedNode::new(value).unwrap().into(),
        "blank" => BlankNode::new(value).unwrap().into(),
        "literal" => Literal::new_simple_literal(value).into(),
        _ => panic!("unknown original golden term kind"),
    });
    GeneralizedQuad::new(subject, predicate, object, GraphName::DefaultGraph)
}

#[test]
fn full_term_algebra_matches_original_equality_round_before_blank_predicate_cleanup() {
    let original: OriginalRound = serde_json::from_str(include_str!(
        "../../tests/fixtures/generalized_equality_original_round.json"
    ))
    .unwrap();
    let mut data =
        oxrdf::GeneralizedDataset::from_iter(original.input.into_iter().map(original_row));
    // bounded: actual synthetic OWL7.6.2 pre-postprocess round, no syntax projection.
    let expected: HashSet<_> = original.expected.into_iter().map(original_row).collect();
    let transaction = data.transaction();
    let reference = one_round(&transaction).unwrap();
    assert_eq!(reference, expected);
    assert_eq!(equality_rules_one_round(&transaction).unwrap(), expected);
    let mut actual = super::Pending::default();
    super::same_as::apply(&transaction, &mut actual).unwrap();
    assert_eq!(actual.0, reference);
    assert!(
        reference
            .iter()
            .any(|q| q.subject.is_literal() && q.predicate.is_blank_node())
    );
}
