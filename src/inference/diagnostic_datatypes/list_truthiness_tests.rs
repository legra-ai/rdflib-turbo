//! Stock RDFLib 7.6.0 node bool/list-item oracle, independent of SPARQL EBV.
use oxigraph::model::*;

use super::list_truthiness::truthy;

#[test]
fn node_truthiness_preserves_empty_names_plain_language_and_default_python_values() {
    for node in [
        Term::NamedNode(NamedNode::new_unchecked("")),
        Term::BlankNode(BlankNode::new_unchecked("")),
        Literal::new_simple_literal("").into(),
        Literal::new_language_tagged_literal("", "en")
            .unwrap()
            .into(),
    ] {
        assert!(!truthy(&node, 4300));
    }
    for (datatype, value, expected) in [
        ("string", "", false),
        ("string", " ", true),
        ("integer", "0", false),
        ("integer", "-00", false),
        ("integer", "invalid", true),
        ("boolean", "false", false),
        ("boolean", "invalid", false),
        ("boolean", " true ", false),
        ("decimal", "0.00", false),
        ("decimal", "NaN", true),
        ("decimal", "sNaN", true),
        ("double", "0.0", false),
        ("double", "NaN", true),
        ("double", "invalid", true),
        ("base64Binary", " ", false),
        ("base64Binary", "!", false),
        ("base64Binary", "=", false),
        ("base64Binary", "A", true),
        ("base64Binary", "AA==", true),
        ("base64Binary", "é", true),
        ("hexBinary", " ", true),
        ("hexBinary", "", false),
        ("hexBinary", "00", true),
        ("normalizedString", "\t", true),
        ("token", "   ", true),
        ("anyURI", "", false),
    ] {
        let node = Literal::new_typed_literal(
            value,
            NamedNode::new(format!("http://www.w3.org/2001/XMLSchema#{datatype}")).unwrap(),
        )
        .into();
        assert_eq!(truthy(&node, 4300), expected, "{datatype} {value:?}");
    }
}

#[test]
fn integer_constructor_limit_is_distinct_from_lexical_zero() {
    let node = Literal::new_typed_literal(
        "0".repeat(641),
        NamedNode::new("http://www.w3.org/2001/XMLSchema#integer").unwrap(),
    )
    .into();
    // RDFLib caches value=None when the default Python constructor hits its
    // configured limit, then bool uses the nonempty lexical form.
    assert!(truthy(&node, 640));
    assert!(!truthy(&node, 0));
}

#[test]
fn duration_truthiness_uses_python_timedelta_rounding_and_not_sparql_ebv() {
    for (value, expected) in [
        ("P0D", false),
        ("PT0S", false),
        ("P0Y", false),
        ("P0M", false),
        ("PT0.0000001S", false),
        ("PT0.0000005S", false),
        ("PT0.0000006S", true),
        ("P", true),
        ("PT", false),
        ("P0DT", false),
        ("P0000-00-00T00:00:00", true),
        ("P0.000000000001D", false),
        ("P1M", true),
    ] {
        let node = Literal::new_typed_literal(
            value,
            NamedNode::new("http://www.w3.org/2001/XMLSchema#duration").unwrap(),
        )
        .into();
        assert_eq!(truthy(&node, 4300), expected, "{value:?}");
    }
}
