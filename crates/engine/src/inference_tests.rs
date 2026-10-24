use super::*;
use oxigraph::io::RdfFormat;

fn data(source: &str) -> Dataset {
    crate::parse_dataset(source, RdfFormat::TriG, None).unwrap()
}

#[test]
fn disabled_and_unknown_inference_modes_preserve_input() {
    let original = data("<urn:s> <urn:p> <urn:o>.");
    for mode in ["none", ""] {
        let mut working = original.clone();
        expand(&mut working, mode).unwrap();
        assert_eq!(working, original);
    }
    assert!(expand(&mut original.clone(), "invalid").is_err());
}

#[test]
fn named_graph_inference_commits_new_union_entailments_only() {
    let mut input = data(
        "<urn:g> { <urn:C> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <urn:D>. <urn:s> a <urn:C>. }",
    );
    expand(&mut input, "rdfs").unwrap();
    assert!(input.contains(QuadRef::new(
        NamedNodeRef::new_unchecked("urn:s"),
        iri(crate::graph::RDF, "type").as_ref(),
        NamedNodeRef::new_unchecked("urn:D"),
        GraphNameRef::DefaultGraph
    )));
    assert!(!input.contains(QuadRef::new(
        NamedNodeRef::new_unchecked("urn:s"),
        iri(crate::graph::RDF, "type").as_ref(),
        NamedNodeRef::new_unchecked("urn:D"),
        NamedNodeRef::new_unchecked("urn:g")
    )));
}

#[test]
fn failed_union_and_destination_inference_never_commit_partial_axioms() {
    let original = data("<urn:g> { <urn:C> <http://www.w3.org/2002/07/owl#hasKey> _:broken. }");
    let mut working = original.clone();
    assert!(expand(&mut working, "owlrl").is_err());
    assert_eq!(working, original);
    assert!(
        expand_into(
            &mut working,
            "owlrl",
            NamedNode::new("urn:inferred").unwrap().into()
        )
        .is_err()
    );
    assert_eq!(working, original);
    for source in [
        "_:predicate <http://www.w3.org/2002/07/owl#propertyChainAxiom> (<urn:p>).",
        "<urn:p> <http://www.w3.org/2002/07/owl#propertyChainAxiom> ('literal').",
        "<urn:C> <http://www.w3.org/2002/07/owl#hasKey> ('literal').",
    ] {
        assert!(expand(&mut data(source), "owlrl").is_err());
    }
}
