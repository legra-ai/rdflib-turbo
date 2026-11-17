use super::{GraphNtOutput, dump_graph_nt};
use oxigraph::model::{GeneralizedQuad, GraphName, Literal, NamedNode, Term};
use oxigraph::store::Store;

#[test]
fn empty_datatype_nt_presentation_does_not_change_stored_identity() {
    for position in 0..3 {
        let store = Store::new().unwrap();
        let mut terms: [Term; 3] = [
            NamedNode::new_unchecked("urn:s").into(),
            NamedNode::new_unchecked("urn:p").into(),
            Literal::new_simple_literal("object").into(),
        ];
        terms[position] = Literal::new_typed_literal("value", NamedNode::new_unchecked("")).into();
        let quad = GeneralizedQuad::new(
            terms[0].clone(),
            terms[1].clone(),
            terms[2].clone(),
            GraphName::DefaultGraph,
        );
        store.insert_generalized(&quad).unwrap();
        let output = dump_graph_nt(store.generalized_quads_for_pattern(
            None,
            None,
            None,
            Some(GraphName::DefaultGraph.as_ref()),
        ))
        .unwrap();
        let GraphNtOutput::Document(text) = output else {
            panic!("Empty datatype is valid reference NT presentation");
        };
        let expected = match position {
            0 => "\"value\" <urn:p> \"object\" .\n",
            1 => "<urn:s> \"value\" \"object\" .\n",
            _ => "<urn:s> <urn:p> \"value\" .\n",
        };
        assert_eq!(text, expected);
        assert!(store.contains_generalized(&quad).unwrap());
    }
}

#[test]
fn literal_n3_in_subject_and_predicate_retains_contextual_reference_formatting() {
    for (literal, n3) in [
        (
            Literal::new_simple_literal("line\nend\""),
            "\"\"\"line\nend\\\"\"\"\"",
        ),
        (
            Literal::new_typed_literal(
                "nan",
                NamedNode::new("http://www.w3.org/2001/XMLSchema#double").unwrap(),
            ),
            "\"NaN\"^^<http://www.w3.org/2001/XMLSchema#double>",
        ),
        (
            Literal::new_typed_literal("text", NamedNode::new_unchecked("urn:datatype space")),
            "\"text\"^^<urn:datatype space>",
        ),
    ] {
        for subject_position in [false, true] {
            let store = Store::new().unwrap();
            let named: Term = NamedNode::new("urn:node").unwrap().into();
            let (subject, predicate) = if subject_position {
                (literal.clone().into(), named)
            } else {
                (named, literal.clone().into())
            };
            store
                .insert_generalized(&GeneralizedQuad::new(
                    subject,
                    predicate,
                    Literal::new_simple_literal("object"),
                    GraphName::DefaultGraph,
                ))
                .unwrap();
            let output = dump_graph_nt(store.generalized_quads_for_pattern(
                None,
                None,
                None,
                Some(GraphName::DefaultGraph.as_ref()),
            ))
            .unwrap();
            let GraphNtOutput::Document(text) = output else {
                panic!("Valid contextual literal must serialize");
            };
            let expected = if subject_position {
                format!("{n3} <urn:node> \"object\" .\n")
            } else {
                format!("<urn:node> {n3} \"object\" .\n")
            };
            assert_eq!(text, expected);
        }
    }
}
