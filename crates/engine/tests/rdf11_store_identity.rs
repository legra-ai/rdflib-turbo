use pyshacl_turbo_engine::oxigraph::io::RdfFormat;
use pyshacl_turbo_engine::oxigraph::model::*;
use pyshacl_turbo_engine::oxigraph::store::Store;
use pyshacl_turbo_engine::{CompiledShapes, Options};

#[test]
fn rdf11_convenience_queries_coalesce_strings_without_changing_exact_owners() {
    let shapes = pyshacl_turbo_engine::IndexedShapesSource::parse_graph(
        r#"@prefix sh: <http://www.w3.org/ns/shacl#>.
        <urn:S> sh:targetNode <urn:s>;
          sh:sparql [sh:select 'SELECT $this WHERE { FILTER NOT EXISTS { $this <urn:value> "value"^^<http://www.w3.org/2001/XMLSchema#string> } }'];
          sh:sparql [sh:select 'SELECT $this WHERE { { SELECT $this (COUNT(?value) AS ?count) WHERE { $this <urn:value> ?value } GROUP BY $this } FILTER (?count != 1) }']."#,
        RdfFormat::Turtle,
        None,
    )
    .unwrap();
    let program = CompiledShapes::compile(shapes, true).unwrap();
    for include_plain in [false, true] {
        let store = Store::new().unwrap();
        let subject = NamedNode::new("urn:s").unwrap();
        let predicate = NamedNode::new("urn:value").unwrap();
        store
            .insert(QuadRef::new(
                &subject,
                &predicate,
                LiteralRef::new_typed_literal(
                    "value",
                    NamedNodeRef::new("http://www.w3.org/2001/XMLSchema#string").unwrap(),
                ),
                GraphNameRef::DefaultGraph,
            ))
            .unwrap();
        if include_plain {
            store
                .insert(QuadRef::new(
                    &subject,
                    &predicate,
                    LiteralRef::new_simple_literal("value"),
                    GraphNameRef::DefaultGraph,
                ))
                .unwrap();
        }
        let length = store.len().unwrap();
        assert_eq!(length, if include_plain { 2 } else { 1 });
        let report = program
            .validate_store(&store, Options::default(), false)
            .unwrap();
        assert!(
            report.conforms,
            "RDF 1.1 view failed for include_plain={include_plain}"
        );
        assert_eq!(store.len().unwrap(), length);
        let transaction = store.start_transaction().unwrap();
        let exact = program
            .validate_readonly(&transaction, Options::default())
            .unwrap();
        assert_eq!(exact.conforms, !include_plain);
    }
}
