use super::*;
use oxigraph::io::RdfFormat;
use oxigraph::sparql::{QueryResults, SparqlEvaluator};

#[test]
fn blank_named_individuals_clone_cyclic_descriptions_and_shared_named_links() {
    let ontology = crate::parse_graph("@prefix owl: <http://www.w3.org/2002/07/owl#>. _:root a owl:NamedIndividual; <urn:friend> <urn:Alice>. <urn:Alice> <urn:friend> _:child. _:child <urn:friend> _:root; <urn:value> 'kept'.", RdfFormat::Turtle, None).unwrap();
    let mut data = Dataset::new();
    inoculate(&mut data, &ontology);
    let QueryResults::Boolean(conforms) = SparqlEvaluator::new().parse_query("ASK { ?root a <http://www.w3.org/2002/07/owl#NamedIndividual>; <urn:friend> <urn:Alice>. <urn:Alice> <urn:friend> ?child. ?child <urn:friend> ?root; <urn:value> 'kept'. }").unwrap().on_queryable_dataset(&data).execute().unwrap() else { panic!("ASK result expected") };
    assert!(conforms);
    assert!(!data.contains(QuadRef::new(
        BlankNodeRef::new("root").unwrap(),
        iri(RDF, "type").as_ref(),
        iri(OWL, "NamedIndividual").as_ref(),
        GraphNameRef::DefaultGraph
    )));
}
