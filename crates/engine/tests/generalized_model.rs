//! Generalized data terms stay native; narrowing to strict RDF is explicit.

use oxigraph::model::{
    BlankNode, GeneralizedQuad, GeneralizedQuadRef, GeneralizedTermPosition, GeneralizedTriple,
    GeneralizedTripleRef, GraphName, Literal, NamedNode, Quad, QuadRef, Term, Triple, TripleRef,
};

fn iri(value: &str) -> NamedNode {
    NamedNode::new(value).unwrap()
}

#[test]
fn generalized_model_preserves_all_data_positions_and_graph_names() {
    // bounded: three RDF term kinds and three supported graph-name forms.
    let terms = [
        Term::from(iri("urn:term")),
        BlankNode::new("term").unwrap().into(),
        Literal::new_language_tagged_literal("term", "en")
            .unwrap()
            .into(),
    ];
    let graphs = [
        GraphName::DefaultGraph,
        iri("urn:graph").into(),
        BlankNode::new("graph").unwrap().into(),
    ];
    for subject in &terms {
        for predicate in &terms {
            for graph in &graphs {
                let quad = GeneralizedQuad::new(
                    subject.clone(),
                    predicate.clone(),
                    terms[2].clone(),
                    graph.clone(),
                );
                assert_eq!(&quad.subject, subject);
                assert_eq!(&quad.predicate, predicate);
                assert_eq!(quad.object, terms[2]);
                assert_eq!(&quad.graph_name, graph);
                assert_eq!(quad.as_ref().into_owned(), quad);
                assert_eq!(GeneralizedQuadRef::from(&quad), quad.as_ref());
                let triple =
                    GeneralizedTriple::new(subject.clone(), predicate.clone(), terms[2].clone());
                assert_eq!(triple.as_ref().into_owned(), triple);
                assert_eq!(GeneralizedTripleRef::from(&triple), triple.as_ref());
            }
        }
    }
}

#[test]
fn strict_promotion_and_checked_narrowing_preserve_exact_terms() {
    let original = Quad::new(
        BlankNode::new("subject").unwrap(),
        iri("urn:predicate"),
        Literal::new_typed_literal("01", iri("http://www.w3.org/2001/XMLSchema#integer")),
        BlankNode::new("graph").unwrap(),
    );
    let promoted = GeneralizedQuad::from(original.clone());
    assert_eq!(
        QuadRef::try_from(promoted.as_ref()).unwrap(),
        original.as_ref()
    );
    assert_eq!(Quad::try_from(promoted).unwrap(), original);
    let borrowed = GeneralizedQuadRef::from(original.as_ref());
    assert_eq!(QuadRef::try_from(borrowed).unwrap(), original.as_ref());
    let original_triple = Triple::new(
        original.subject.clone(),
        original.predicate.clone(),
        original.object.clone(),
    );
    let promoted_triple = GeneralizedTriple::from(original_triple.clone());
    assert_eq!(
        TripleRef::try_from(promoted_triple.as_ref()).unwrap(),
        original_triple.as_ref()
    );
    assert_eq!(Triple::try_from(promoted_triple).unwrap(), original_triple);
    assert_eq!(
        TripleRef::try_from(GeneralizedTripleRef::from(original_triple.as_ref())).unwrap(),
        original_triple.as_ref()
    );
}

#[test]
fn strict_narrowing_errors_identify_first_invalid_position() {
    let literal = Term::from(Literal::new_simple_literal("literal"));
    let blank = Term::from(BlankNode::new("predicate").unwrap());
    for predicate in [
        Term::from(iri("urn:predicate")),
        blank.clone(),
        literal.clone(),
    ] {
        let quad = GeneralizedQuad::new(
            literal.clone(),
            predicate,
            iri("urn:object"),
            GraphName::DefaultGraph,
        );
        let triple = GeneralizedTriple::new(
            quad.subject.clone(),
            quad.predicate.clone(),
            quad.object.clone(),
        );
        for error in [
            QuadRef::try_from(quad.as_ref()).unwrap_err(),
            Quad::try_from(quad).unwrap_err(),
            TripleRef::try_from(triple.as_ref()).unwrap_err(),
            Triple::try_from(triple).unwrap_err(),
        ] {
            assert_eq!(error.position(), GeneralizedTermPosition::Subject);
            assert_eq!(
                error.to_string(),
                "RDF subject must be an IRI or blank node"
            );
        }
    }
    for predicate in [blank, literal] {
        let triple = GeneralizedTriple::new(iri("urn:subject"), predicate, iri("urn:object"));
        let quad = GeneralizedQuad::new(
            triple.subject.clone(),
            triple.predicate.clone(),
            triple.object.clone(),
            GraphName::DefaultGraph,
        );
        for error in [
            TripleRef::try_from(triple.as_ref()).unwrap_err(),
            Triple::try_from(triple).unwrap_err(),
            QuadRef::try_from(quad.as_ref()).unwrap_err(),
            Quad::try_from(quad).unwrap_err(),
        ] {
            assert_eq!(error.position(), GeneralizedTermPosition::Predicate);
            assert_eq!(error.to_string(), "RDF predicate must be an IRI");
        }
    }
}

#[test]
fn generalized_subjects_keep_lexical_datatype_language_and_plain_string_identity() {
    let integer = iri("http://www.w3.org/2001/XMLSchema#integer");
    // bounded: six explicit literal identity controls, never a source relation.
    let terms = [
        Literal::new_typed_literal("01", integer.clone()),
        Literal::new_typed_literal("1", integer),
        Literal::new_simple_literal("text"),
        Literal::new_typed_literal("text", iri("http://www.w3.org/2001/XMLSchema#string")),
        Literal::new_language_tagged_literal("text", "en").unwrap(),
        Literal::new_language_tagged_literal("text", "nl").unwrap(),
    ];
    for (left_index, left) in terms.iter().enumerate() {
        for (right_index, right) in terms.iter().enumerate() {
            let left = GeneralizedQuad::new(
                left.clone(),
                iri("urn:p"),
                iri("urn:o"),
                GraphName::DefaultGraph,
            );
            let right = GeneralizedQuad::new(
                right.clone(),
                iri("urn:p"),
                iri("urn:o"),
                GraphName::DefaultGraph,
            );
            assert_eq!(left == right, left_index == right_index);
            assert_eq!(left.as_ref() == right.as_ref(), left_index == right_index);
        }
    }
}
