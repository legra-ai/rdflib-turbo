use super::*;
use oxrdf::GeneralizedDataset;

#[test]
fn initial_resource_seeding_retains_every_literal_object() {
    for literal in [
        Literal::new_simple_literal("record"),
        Literal::new_language_tagged_literal("record", "en").unwrap(),
        Literal::new_typed_literal("record", iri(XSD, "string")),
        Literal::new_typed_literal("7", iri(XSD, "integer")),
    ] {
        let mut data = GeneralizedDataset::from_iter([GeneralizedQuad::new(
            iri("urn:", "subject"),
            iri("urn:", "label"),
            literal.clone(),
            GraphName::DefaultGraph,
        )]);
        let transaction = data.transaction();
        let actual = resources(&transaction)
            .collect::<Result<HashSet<_>>>()
            .unwrap();
        assert_eq!(
            actual,
            HashSet::from([
                triple(
                    iri("urn:", "subject"),
                    iri(RDF, "type"),
                    iri(RDFS, "Resource")
                ),
                triple(literal, iri(RDF, "type"), iri(RDFS, "Resource")),
            ])
        );
    }
}

#[test]
fn used_datatypes_receive_all_ancestor_types_and_disjointness_axioms() {
    let mut data = GeneralizedDataset::new();
    for name in XSD_TYPES {
        let node = iri("urn:datatype:", name);
        data.insert(&GeneralizedQuad::new(
            node.clone(),
            iri(RDF, "type"),
            iri(XSD, name),
            GraphName::DefaultGraph,
        ));
        data.insert(&GeneralizedQuad::new(
            node,
            iri(OWL, "sameAs"),
            Literal::new_typed_literal("1", iri(XSD, name)),
            GraphName::DefaultGraph,
        ));
    }
    data.insert(&GeneralizedQuad::new(
        BlankNode::default(),
        iri("urn:", "value"),
        Literal::new_language_tagged_literal("text", "en").unwrap(),
        GraphName::DefaultGraph,
    ));
    for name in ["PlainLiteral", "XMLLiteral"] {
        data.insert(&GeneralizedQuad::new(
            iri("urn:", name),
            iri(OWL, "sameAs"),
            Literal::new_typed_literal("text", iri(RDF, name)),
            GraphName::DefaultGraph,
        ));
    }
    data.insert(&GeneralizedQuad::new(
        iri("urn:", "named"),
        iri("urn:", "link"),
        BlankNode::default(),
        GraphName::DefaultGraph,
    ));
    for combined in [false, true] {
        let mut expanded = data.clone();
        let mut transaction = expanded.transaction();
        owl(
            &mut transaction,
            combined,
            &mut Diagnostics::new(Vec::new(), 4300),
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
        let transaction = data.transaction();
        assert!(resources(&transaction).next().is_some());
    }
}

#[test]
fn implicit_literals_receive_own_datatype_types_without_typing_plain_or_language_strings() {
    for (literal, types) in [
        (Literal::new_simple_literal("plain"), &[][..]),
        (
            Literal::new_language_tagged_literal("language", "en").unwrap(),
            &[][..],
        ),
        (
            Literal::new_typed_literal("typed", iri(XSD, "string")),
            &["string"][..],
        ),
        (
            Literal::new_typed_literal("7", iri(XSD, "integer")),
            &["integer", "decimal"][..],
        ),
    ] {
        let mut data = GeneralizedDataset::from_iter([GeneralizedQuad::new(
            iri("urn:", "subject"),
            iri("urn:", "value"),
            literal.clone(),
            GraphName::DefaultGraph,
        )]);
        let mut transaction = data.transaction();
        owl(
            &mut transaction,
            false,
            &mut Diagnostics::new(Vec::new(), 4300),
            &LexicalContext::default(),
        )
        .unwrap();
        transaction.commit();
        let actual = data
            .quads_for_pattern(
                Some(literal.as_ref().into()),
                Some(iri(RDF, "type").as_ref().into()),
                None,
                Some(GraphNameRef::DefaultGraph),
            )
            .map(|q| q.object.into_owned())
            .collect::<HashSet<_>>();
        let expected = types
            .iter()
            .map(|name| Term::from(iri(XSD, name)))
            .collect::<HashSet<_>>();
        assert_eq!(actual, expected, "implicit datatype subject {literal:?}");
    }
}

#[test]
fn datatype_aliases_receive_original_subsumptions_without_invented_base_membership() {
    for (literal, types) in [
        (Literal::new_simple_literal("plain"), &[][..]),
        (
            Literal::new_language_tagged_literal("language", "en").unwrap(),
            &[][..],
        ),
        (
            Literal::new_typed_literal("typed", iri(XSD, "string")),
            &[(RDFS, "Datatype")][..],
        ),
        (
            Literal::new_typed_literal("7", iri(XSD, "integer")),
            &[(RDFS, "Datatype"), (XSD, "decimal")][..],
        ),
    ] {
        let alias = iri("urn:", "alias");
        let mut data = GeneralizedDataset::from_iter([GeneralizedQuad::new(
            alias.clone(),
            iri(OWL, "sameAs"),
            literal,
            GraphName::DefaultGraph,
        )]);
        let mut transaction = data.transaction();
        owl(
            &mut transaction,
            false,
            &mut Diagnostics::new(Vec::new(), 4300),
            &LexicalContext::default(),
        )
        .unwrap();
        transaction.commit();
        let actual = data
            .quads_for_pattern(
                Some(alias.as_ref().into()),
                Some(iri(RDF, "type").as_ref().into()),
                None,
                Some(GraphNameRef::DefaultGraph),
            )
            .map(|q| q.object.into_owned())
            .collect::<HashSet<_>>();
        let expected = types
            .iter()
            .map(|(namespace, name)| Term::from(iri(namespace, name)))
            .collect::<HashSet<_>>();
        assert_eq!(actual, expected, "one-time alias typing");
    }
}
