//! Malformed constraint declarations fail as pySHACL 0.40.2 `ConstraintLoadError`s.

use super::*;

fn load_error(result: Result<IntegerParameter>) -> (&'static str, &'static str) {
    match result {
        Err(Error::ConstraintLoad { message, link }) => (message, link),
        Err(other) => panic!("expected ConstraintLoad, got {other:?}"),
        Ok(_) => panic!("expected ConstraintLoad, got a parameter"),
    }
}

fn typed(value: &str, datatype: &str) -> Term {
    Literal::new_typed_literal(value, iri(XSD, datatype)).into()
}

#[test]
fn malformed_integer_parameters_raise_component_load_errors() {
    let not_literal: Term = iri("urn:", "notLiteral").into();
    let cases: [(&str, Term, &str, &str); 18] = [
        (
            "minLength",
            typed("5", "negativeInteger"),
            "sh:minLength value must be a literal value with an integer.",
            "https://www.w3.org/TR/shacl/#MinLengthConstraintComponent",
        ),
        (
            "maxLength",
            typed("300", "unsignedByte"),
            "sh:maxLength value must be a literal value with an integer.",
            "https://www.w3.org/TR/shacl/#MaxLengthConstraintComponent",
        ),
        (
            "minCount",
            not_literal.clone(),
            "MinCountConstraintComponent sh:minCount must be a literal with datatype xsd:integer.",
            "https://www.w3.org/TR/shacl/#MinCountConstraintComponent",
        ),
        (
            "minCount",
            typed("1", "int"),
            "MinCountConstraintComponent sh:minCount must be a literal with datatype xsd:integer.",
            "https://www.w3.org/TR/shacl/#MinCountConstraintComponent",
        ),
        (
            "minCount",
            typed("-1", "integer"),
            "MinCountConstraintComponent sh:minCount must be an integer >= 0.",
            "https://www.w3.org/TR/shacl/#MinCountConstraintComponent",
        ),
        (
            "maxCount",
            Literal::new_simple_literal("1").into(),
            "MaxCountConstraintComponent sh:maxCount must be a literal with datatype xsd:integer.",
            "https://www.w3.org/TR/shacl/#MaxCountConstraintComponent",
        ),
        (
            "maxCount",
            typed("-2", "integer"),
            "MaxCountConstraintComponent sh:maxCount must be an integer >= 0.",
            "https://www.w3.org/TR/shacl/#MaxCountConstraintComponent",
        ),
        (
            "minLength",
            not_literal.clone(),
            "sh:minLength value must be a literal value with an integer.",
            "https://www.w3.org/TR/shacl/#MinLengthConstraintComponent",
        ),
        (
            "minLength",
            typed("1", "byte"),
            "sh:minLength value must be a literal value with an integer.",
            "https://www.w3.org/TR/shacl/#MinLengthConstraintComponent",
        ),
        (
            "minLength",
            typed("-1", "integer"),
            "sh:minLength value must be a positive integer.",
            "https://www.w3.org/TR/shacl/#MinLengthConstraintComponent",
        ),
        (
            "minLength",
            typed("-1", "negativeInteger"),
            "sh:minLength value must be a positive integer.",
            "https://www.w3.org/TR/shacl/#MinLengthConstraintComponent",
        ),
        (
            "maxLength",
            typed("1.5", "decimal"),
            "sh:maxLength value must be a literal value with an integer.",
            "https://www.w3.org/TR/shacl/#MaxLengthConstraintComponent",
        ),
        (
            "maxLength",
            typed("0", "nonPositiveInteger"),
            "sh:maxLength value must be a positive integer.",
            "https://www.w3.org/TR/shacl/#MaxLengthConstraintComponent",
        ),
        (
            "maxLength",
            typed("-3", "int"),
            "sh:maxLength value must be a positive integer.",
            "https://www.w3.org/TR/shacl/#MaxLengthConstraintComponent",
        ),
        (
            "qualifiedMinCount",
            not_literal.clone(),
            "QualifiedMinCountConstraintComponent sh:qualifiedMinCount must be a Literal with Int.",
            "https://www.w3.org/TR/shacl/#QualifiedValueShapeConstraintComponent",
        ),
        (
            "qualifiedMinCount",
            typed("1.0", "decimal"),
            "QualifiedMinCountConstraintComponent sh:qualifiedMinCount must be a Literal with Int.",
            "https://www.w3.org/TR/shacl/#QualifiedValueShapeConstraintComponent",
        ),
        (
            "qualifiedMaxCount",
            typed("maybe", "boolean"),
            "QualifiedMaxCountConstraintComponent sh:qualifiedMaxCount must be a Literal with Int.",
            "https://www.w3.org/TR/shacl/#QualifiedValueShapeConstraintComponent",
        ),
        (
            "qualifiedMaxCount",
            not_literal,
            "QualifiedMaxCountConstraintComponent sh:qualifiedMaxCount must be a Literal with Int.",
            "https://www.w3.org/TR/shacl/#QualifiedValueShapeConstraintComponent",
        ),
    ];
    for (name, term, message, link) in cases {
        assert_eq!(load_error(integer(term, name)), (message, link), "{name}");
    }
}

#[test]
fn out_of_range_qualified_counts_load_like_the_reference() {
    let parameter = integer(typed("0", "positiveInteger"), "qualifiedMinCount").unwrap();
    assert_eq!(parameter.number, BigInt::from(0));
}

#[test]
fn ignored_properties_without_closed_raise_closed_component_load_error() {
    let shape = NamedNode::new("urn:closed:shape").unwrap();
    let data = Dataset::from_iter([Quad::new(
        shape.clone(),
        iri(SH, "ignoredProperties"),
        iri(RDF, "nil"),
        GraphName::DefaultGraph,
    )]);
    match read(&data, &Term::from(shape), false, Default::default()) {
        Err(Error::ConstraintLoad { message, link }) => {
            assert_eq!(
                message,
                "ClosedConstraintComponent: You can only use sh:ignoredProperties on a Closed \
                 Shape (sh:closed)."
            );
            assert_eq!(
                link,
                "https://www.w3.org/TR/shacl/#ClosedConstraintComponent"
            );
        }
        other => panic!("expected ConstraintLoad, got {other:?}"),
    }
}
