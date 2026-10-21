use super::*;

#[test]
fn used_datatypes_receive_all_ancestor_types_and_disjointness_axioms() {
    let mut data = Dataset::new();
    for name in XSD_TYPES {
        let node = iri("urn:datatype:", name);
        data.insert(&Quad::new(
            node.clone(),
            iri(RDF, "type"),
            iri(XSD, name),
            GraphName::DefaultGraph,
        ));
        data.insert(&Quad::new(
            node,
            iri(OWL, "sameAs"),
            Literal::new_typed_literal("1", iri(XSD, name)),
            GraphName::DefaultGraph,
        ));
    }
    data.insert(&Quad::new(
        BlankNode::default(),
        iri("urn:", "value"),
        Literal::new_language_tagged_literal("text", "en").unwrap(),
        GraphName::DefaultGraph,
    ));
    for name in ["PlainLiteral", "XMLLiteral"] {
        data.insert(&Quad::new(
            iri("urn:", name),
            iri(OWL, "sameAs"),
            Literal::new_typed_literal("text", iri(RDF, name)),
            GraphName::DefaultGraph,
        ));
    }
    data.insert(&Quad::new(
        iri("urn:", "named"),
        iri("urn:", "link"),
        BlankNode::default(),
        GraphName::DefaultGraph,
    ));
    for combined in [false, true] {
        let mut expanded = data.clone();
        let mut transaction = expanded.insert_transaction();
        owl(
            &mut transaction,
            combined,
            &mut Diagnostics::default(),
            &LexicalContext::default(),
        )
        .unwrap();
        transaction.commit();
        assert!(expanded.contains(QuadRef::new(
            iri("urn:datatype:", "unsignedByte").as_ref(),
            iri(RDF, "type").as_ref(),
            iri(XSD, "integer").as_ref(),
            GraphNameRef::DefaultGraph
        )));
        assert!(expanded.contains(QuadRef::new(
            iri(XSD, "decimal").as_ref(),
            iri(OWL, "disjointWith").as_ref(),
            iri(XSD, "string").as_ref(),
            GraphNameRef::DefaultGraph
        )));
        assert!(!expanded.contains(QuadRef::new(
            iri(XSD, "string").as_ref(),
            iri(OWL, "disjointWith").as_ref(),
            iri(RDF, "PlainLiteral").as_ref(),
            GraphNameRef::DefaultGraph
        )));
        let transaction = data.insert_transaction();
        assert!(resources(&transaction).next().is_some());
    }
}
