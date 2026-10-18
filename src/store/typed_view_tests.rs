use oxigraph::model::TermRef;

use super::{Position, PreparedTerm, TermView};
use crate::store::typed;

#[test]
fn prepared_literal_borrows_lexical_and_normalizes_only_language() {
    // bounded: one large lexical proving borrowed preparation does not duplicate its payload.
    let value = "日本語".repeat(4096);
    for language in [None, Some("fr"), Some("fR")] {
        let prepared = PreparedTerm::new(
            TermView::new("literal", &value, None, language),
            Position::Object,
        )
        .unwrap();
        let TermRef::Literal(literal) = prepared.term_ref() else {
            panic!("expected literal")
        };
        assert_eq!(literal.value().as_ptr(), value.as_ptr());
        assert_eq!(literal.language(), language.map(|_| "fr"));
    }
}

#[test]
fn owned_and_borrowed_inputs_share_validation_errors_and_language_semantics() {
    // bounded: fixed input forms compare both ownership routes through one validator.
    for (kind, value, datatype, language) in [
        ("uri", "relative", None, None),
        ("bnode", "0123", None, None),
        (
            "literal",
            "01",
            Some("http://www.w3.org/2001/XMLSchema#integer"),
            None,
        ),
        ("literal", "x", None, Some("FR")),
        ("literal", "x", None, Some("not valid")),
        ("literal", "x", Some("urn:type"), Some("en")),
        ("uri", "x", None, Some("en")),
        ("unknown", "x", None, None),
    ] {
        let owned = typed::term((
            kind.into(),
            value.into(),
            datatype.map(str::to_owned),
            language.map(str::to_owned),
        ));
        let borrowed = PreparedTerm::new(
            TermView::new(kind, value, datatype, language),
            Position::Object,
        );
        match (owned, borrowed) {
            (Ok(owned), Ok(borrowed)) => assert_eq!(owned.as_ref(), borrowed.term_ref()),
            (Err(owned), Err(borrowed)) => assert_eq!(owned.to_string(), borrowed.to_string()),
            _ => panic!("ownership routes disagree"),
        }
    }
}
