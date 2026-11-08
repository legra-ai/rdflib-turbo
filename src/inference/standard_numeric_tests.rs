//! Shared numeric rule execution through real generalized inference transactions.
use super::{InferenceRequest, expand};
use crate::graph::{RDF, XSD, iri};
use oxigraph::model::*;
use oxrdf::GeneralizedDataset;

const MODE: &str = "owl2-rl-numeric-core";
fn value(lexical: &str, datatype: &str) -> Term {
    Literal::new_typed_literal(lexical, iri(XSD, datatype)).into()
}
fn assertion(
    subject: impl Into<Term>,
    predicate: &str,
    object: impl Into<Term>,
) -> GeneralizedQuad {
    GeneralizedQuad::new(
        subject,
        NamedNode::new(predicate).unwrap(),
        object,
        GraphName::DefaultGraph,
    )
}

#[test]
fn standard_numeric_executes_shared_identity_membership_and_diagnostic_rules() {
    let mut data = GeneralizedDataset::new();
    let integer = value("01", "integer");
    let decimal = value("1.0", "decimal");
    let float = value("1.0", "float");
    for (index, literal) in [&integer, &decimal, &float].into_iter().enumerate() {
        data.insert(
            assertion(iri("urn:", &format!("s{index}")), "urn:p", literal.clone()).as_ref(),
        );
    }
    data.insert(assertion(float.clone(), &format!("{RDF}type"), iri(XSD, "integer")).as_ref());
    let outcome = expand(&mut data, MODE, InferenceRequest::default()).unwrap();
    assert!(
        data.contains(
            assertion(
                integer.clone(),
                "http://www.w3.org/2002/07/owl#sameAs",
                decimal.clone()
            )
            .as_ref()
        )
    );
    assert!(
        data.contains(
            assertion(
                integer.clone(),
                "http://www.w3.org/2002/07/owl#differentFrom",
                float.clone()
            )
            .as_ref()
        )
    );
    assert!(
        data.contains(
            assertion(
                integer,
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                iri(XSD, "unsignedByte")
            )
            .as_ref()
        )
    );
    assert_eq!(outcome.error_messages.len(), 1);
    assert!(outcome.error_messages[0].contains("numeric/dt-not-type"));
}

#[test]
fn standard_numeric_invalid_lexical_input_aborts_without_partial_additions() {
    let mut data = GeneralizedDataset::new();
    data.insert(assertion(iri("urn:", "valid"), "urn:p", value("1", "integer")).as_ref());
    data.insert(assertion(iri("urn:", "bad"), "urn:p", value("256", "unsignedByte")).as_ref());
    let before = data.len();
    let error = expand(&mut data, MODE, InferenceRequest::default()).unwrap_err();
    assert!(error.to_string().contains("unsignedByte"), "{error}");
    assert_eq!(data.len(), before);
}

#[test]
fn standard_numeric_preserves_nan_identity_and_signed_zero_difference() {
    let mut data = GeneralizedDataset::new();
    for (index, literal) in [
        value("NaN", "double"),
        value(" NaN ", "double"),
        value("0", "float"),
        value("-0", "float"),
    ]
    .into_iter()
    .enumerate()
    {
        data.insert(assertion(iri("urn:", &format!("s{index}")), "urn:p", literal).as_ref());
    }
    expand(&mut data, MODE, InferenceRequest::default()).unwrap();
    assert!(
        data.contains(
            assertion(
                value("NaN", "double"),
                "http://www.w3.org/2002/07/owl#sameAs",
                value(" NaN ", "double")
            )
            .as_ref()
        )
    );
    assert!(
        data.contains(
            assertion(
                value("0", "float"),
                "http://www.w3.org/2002/07/owl#differentFrom",
                value("-0", "float")
            )
            .as_ref()
        )
    );
}
