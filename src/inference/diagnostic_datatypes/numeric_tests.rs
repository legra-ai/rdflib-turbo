use super::*;
use oxigraph::model::NamedNode;
use serde::Deserialize;

#[derive(Deserialize)]
struct TermFixture {
    datatype: String,
    lexical: String,
}
#[derive(Deserialize)]
struct PairFixture {
    left: usize,
    right: usize,
    invalid_trap: bool,
    expected: String,
}

#[test]
fn default_numeric_keys_match_actual_rdflib_value_pairs() {
    // bounded: fixed 26-term differential regression vocabulary.
    let terms: Vec<TermFixture> =
        serde_json::from_str(include_str!("../numeric-reference-terms.json")).unwrap();
    // bounded: four fixed pair fixture sets.
    for file in [
        include_str!("../numeric-reference-pairs-0.jsonl"),
        include_str!("../numeric-reference-pairs-1.jsonl"),
        include_str!("../numeric-reference-pairs-2.jsonl"),
        include_str!("../numeric-reference-pairs-3.jsonl"),
    ] {
        for line in file.lines() {
            let pair: PairFixture = serde_json::from_str(line).unwrap();
            let context = {
                let mut context = LexicalContext::default();
                context.decimal.traps[0] = pair.invalid_trap;
                context
            };
            let parse = |index: usize| {
                let term = &terms[index];
                parse_numeric(
                    &Literal::new_typed_literal(
                        term.lexical.clone(),
                        NamedNode::new_unchecked(term.datatype.clone()),
                    ),
                    &context,
                )
            };
            let (left, right) = (parse(pair.left), parse(pair.right));
            if pair.expected == "not_numeric" {
                assert!(left.is_none() || right.is_none());
                continue;
            }
            let (Some(left), Some(right)) = (left, right) else {
                panic!(
                    "Missing numeric parser for pair {} / {}",
                    pair.left, pair.right
                );
            };
            let expected = match pair.expected.as_str() {
                "missing" => Ok(None),
                "true" => Ok(Some(true)),
                "false" => Ok(Some(false)),
                "InvalidComparison" => Err(LexicalFailure::DecimalInvalidComparison),
                other => panic!("unknown oracle result {other}"),
            };
            assert_eq!(
                numeric_equal(&left, &right, &context),
                expected,
                "pair {} / {} invalidtrap={}",
                pair.left,
                pair.right,
                pair.invalid_trap
            );
        }
    }
}
