//! Generalized shapes use the actual indexed snapshot and its logical scope.

use std::collections::HashSet;

use oxigraph::io::RdfFormat;
use oxigraph::model::*;
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;
use pyshacl_turbo_engine::{CompiledShapes, GraphRead, IndexedShapesSource, Options, QueryGraph};
use spareval::QueryableDataset;

fn iri(value: &str) -> NamedNode {
    NamedNode::new(value).unwrap()
}

#[test]
fn literal_shapes_compile_from_their_pinned_full_term_source() {
    let store = Store::new().unwrap();
    let shape: Term = Literal::new_simple_literal("shape").into();
    for (predicate, object) in [
        ("targetNode", Term::from(iri("urn:focus"))),
        ("class", Term::from(iri("urn:Expected"))),
    ] {
        store
            .insert_generalized(&GeneralizedQuad::new(
                shape.clone(),
                iri(&format!("http://www.w3.org/ns/shacl#{predicate}")),
                object,
                iri("urn:selected"),
            ))
            .unwrap();
    }
    // Unrelated generalized metadata must survive ownership without becoming a
    // shape declaration or narrowing failure.
    store
        .insert_generalized(&GeneralizedQuad::new(
            Literal::new_simple_literal("metadata"),
            BlankNode::new("annotation").unwrap(),
            Literal::new_simple_literal("value"),
            iri("urn:selected"),
        ))
        .unwrap();
    let source = IndexedShapesSource::new(store.read_snapshot(), Some(iri("urn:selected").into()));
    let program = CompiledShapes::compile(source, false).unwrap();
    drop(store);
    assert_eq!(program.shape_nodes().collect::<Vec<_>>(), [&shape]);
    let result = program
        .validate(&mut Dataset::new(), Options::default())
        .unwrap();
    assert!(!result.conforms);
    assert!(
        result
            .graph
            .quads_for_pattern(
                None,
                Some(iri("http://www.w3.org/ns/shacl#sourceShape").as_ref()),
                Some(shape.as_ref()),
                None,
            )
            .next()
            .is_some()
    );
}

fn ask(source: &impl GraphRead, query: &str) -> bool {
    let result = SparqlEvaluator::new()
        .parse_query(query)
        .unwrap()
        .on_queryable_dataset(QueryGraph::new(source))
        .execute()
        .unwrap();
    let QueryResults::Boolean(value) = result else {
        panic!("expected ASK result")
    };
    value
}

#[test]
fn parsed_shapes_stream_exact_terms_into_their_owned_indexed_source() {
    let text =
        "<urn:s> <urn:p> \"0001\"^^<urn:exact> <urn:a> .\n<urn:s> <urn:p> \"value\" <urn:b> .";
    let source = IndexedShapesSource::parse(text, RdfFormat::NQuads, None, None).unwrap();
    assert!(ask(
        &source,
        "ASK { GRAPH <urn:a> { <urn:s> <urn:p> \"0001\"^^<urn:exact> } }"
    ));
    let selected =
        IndexedShapesSource::parse(text, RdfFormat::NQuads, None, Some(iri("urn:a").into()))
            .unwrap();
    assert_eq!(selected.matching_quads(None, None, None, None).count(), 1);
    assert!(!ask(&selected, "ASK { GRAPH <urn:b> { ?s ?p ?o } }"));
    assert!(
        IndexedShapesSource::parse(
            "<urn:s> <urn:p> \"unterminated",
            RdfFormat::NTriples,
            None,
            None,
        )
        .is_err()
    );
    assert!(
        IndexedShapesSource::parse("", RdfFormat::Turtle, Some("relative-base"), None,).is_err()
    );
}

#[test]
fn selected_shapes_pin_full_terms_and_do_not_expose_other_contexts() {
    for scope in [
        GraphName::DefaultGraph,
        iri("urn:shapes").into(),
        BlankNode::new("shapes").unwrap().into(),
    ] {
        let store = Store::new().unwrap();
        let subject: Term = Literal::new_simple_literal("shape").into();
        let predicate: Term = BlankNode::new("predicate").unwrap().into();
        let object: Term = Literal::new_typed_literal("0001", iri("urn:datatype:exact")).into();
        let original = GeneralizedQuad::new(
            subject.clone(),
            predicate.clone(),
            object.clone(),
            scope.clone(),
        );
        store.insert_generalized(&original).unwrap();
        store
            .insert_generalized(&GeneralizedQuad::new(
                subject.clone(),
                predicate.clone(),
                Literal::new_simple_literal("unrelated"),
                iri("urn:unrelated"),
            ))
            .unwrap();
        let source = IndexedShapesSource::new(store.read_snapshot(), Some(scope.clone()));
        store.remove_generalized(&original).unwrap();
        store
            .insert_generalized(&GeneralizedQuad::new(
                subject.clone(),
                predicate.clone(),
                Literal::new_simple_literal("later"),
                scope,
            ))
            .unwrap();
        drop(store);
        // bounded: the selected synthetic source contains exactly one row.
        let rows: Vec<_> = source
            .matching_quads(
                Some(subject.as_ref()),
                Some(predicate.as_ref()),
                Some(object.as_ref()),
                Some(GraphNameRef::DefaultGraph),
            )
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            rows,
            [GeneralizedQuad::new(
                subject.clone(),
                predicate.clone(),
                object.clone(),
                GraphName::DefaultGraph,
            )]
        );
        assert_eq!(source.matching_quads(None, None, None, None).count(), 1);
        assert_eq!(source.named_graphs().count(), 0);
        assert!(
            !source
                .contains_named_graph(&iri("urn:unrelated").into())
                .unwrap()
        );
        assert!(!ask(&source, "ASK { GRAPH <urn:unrelated> { ?s ?p ?o } }"));

        let query = QueryGraph::new(&source);
        // Bound generalized positions must reach storage, rather than becoming
        // an empty answer in the SPARQL dataset adapter.
        let row = query
            .internal_quads_for_pattern(Some(&subject), Some(&predicate), Some(&object), Some(None))
            .next()
            .unwrap()
            .unwrap();
        assert_eq!(
            (row.subject, row.predicate, row.object),
            (subject, predicate, object)
        );
    }
}

#[test]
fn dataset_shapes_preserve_union_named_contexts_and_empty_graphs() {
    let store = Store::new().unwrap();
    for graph in [
        GraphName::DefaultGraph,
        iri("urn:a").into(),
        iri("urn:b").into(),
    ] {
        store
            .insert_generalized(&GeneralizedQuad::new(
                iri("urn:s"),
                iri("urn:p"),
                Literal::new_simple_literal("value"),
                graph,
            ))
            .unwrap();
    }
    store
        .insert_generalized(&GeneralizedQuad::new(
            Literal::new_simple_literal("shape"),
            Literal::new_simple_literal("predicate"),
            Literal::new_simple_literal("metadata"),
            iri("urn:c"),
        ))
        .unwrap();
    {
        let mut transaction = store.start_transaction().unwrap();
        transaction.insert_named_graph(&iri("urn:empty"));
        transaction.commit().unwrap();
    }
    let source = IndexedShapesSource::new(store.read_snapshot(), None);
    drop(store);
    // bounded: two union rows and four graph names in this synthetic fixture.
    let defaults: HashSet<_> = source
        .matching_quads(None, None, None, Some(GraphNameRef::DefaultGraph))
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(defaults.len(), 2);
    assert_eq!(source.matching_quads(None, None, None, None).count(), 5);
    let names: HashSet<_> = source.named_graphs().collect::<Result<_, _>>().unwrap();
    assert_eq!(names.len(), 4);
    assert!(
        source
            .contains_named_graph(&iri("urn:empty").into())
            .unwrap()
    );
    assert!(ask(&source, "ASK { <urn:s> <urn:p> \"value\" }"));
    assert!(ask(
        &source,
        "ASK { GRAPH <urn:a> { <urn:s> <urn:p> \"value\" } }"
    ));
    assert!(ask(&source, "ASK { GRAPH <urn:empty> {} }"));
}

#[test]
fn equal_shape_unions_do_not_erase_named_graph_placement() {
    let left = Store::new().unwrap();
    let right = Store::new().unwrap();
    for (store, graph) in [(&left, "urn:a"), (&right, "urn:b")] {
        store
            .insert_generalized(&GeneralizedQuad::new(
                iri("urn:s"),
                iri("urn:p"),
                Literal::new_simple_literal("value"),
                iri(graph),
            ))
            .unwrap();
    }
    let left = IndexedShapesSource::new(left.read_snapshot(), None);
    let right = IndexedShapesSource::new(right.read_snapshot(), None);
    // bounded: each fixture union has exactly one row.
    let left_union: Vec<_> = left
        .matching_quads(None, None, None, Some(GraphNameRef::DefaultGraph))
        .collect::<Result<_, _>>()
        .unwrap();
    let right_union: Vec<_> = right
        .matching_quads(None, None, None, Some(GraphNameRef::DefaultGraph))
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(left_union, right_union);
    let query = "ASK { GRAPH <urn:a> { <urn:s> <urn:p> \"value\" } }";
    assert!(ask(&left, query));
    assert!(!ask(&right, query));
}

#[test]
fn generalized_literal_focus_reads_are_not_silently_skipped_by_closed_shapes() {
    let shapes = IndexedShapesSource::parse(
        "@prefix sh: <http://www.w3.org/ns/shacl#>. <urn:S> sh:targetNode 'focus'; sh:closed true.",
        RdfFormat::Turtle,
        None,
        Some(GraphName::DefaultGraph),
    )
    .unwrap();
    let compiled = CompiledShapes::compile(shapes, false).unwrap();
    for predicate in [
        Term::from(iri("urn:unexpected")),
        BlankNode::new("unexpected").unwrap().into(),
        Literal::new_simple_literal("unexpected").into(),
    ] {
        let store = Store::new().unwrap();
        store
            .insert_generalized(&GeneralizedQuad::new(
                Literal::new_simple_literal("focus"),
                predicate,
                iri("urn:o"),
                GraphName::DefaultGraph,
            ))
            .unwrap();
        let data = store.read_snapshot();
        assert!(
            !compiled
                .validate_store(&store, Options::default(), false)
                .unwrap()
                .conforms
        );
        drop(store);
        let result = compiled.validate_delta(&data, Options::default()).unwrap();
        assert!(!result.report.conforms);
        assert!(result.additions.is_empty());
    }
}
