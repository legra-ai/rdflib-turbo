use super::*;
use oxigraph::io::RdfFormat;
use oxigraph::store::Store;

#[test]
fn readonly_validation_uses_pending_named_graph_changes_and_borrowed_shapes_overlay() {
    let shapes = crate::IndexedShapesSource::parse_graph(r#"@prefix sh: <http://www.w3.org/ns/shacl#> .
        <urn:has> a sh:SPARQLFunction; sh:ask "ASK { <urn:s> <urn:p> ?value }" .
        <urn:S> sh:targetNode <urn:s>;
          sh:property [sh:path <urn:p>; sh:minCount 1];
          sh:sparql [sh:select "SELECT $this WHERE { GRAPH $shapesGraph { $currentShape <http://www.w3.org/ns/shacl#targetNode> $this } FILTER(!<urn:has>()) }"] ."#, RdfFormat::Turtle, None).unwrap();
    let program = CompiledShapes::compile(shapes, true).unwrap();
    let store = Store::new().unwrap();
    let mut transaction = store.start_transaction().unwrap();
    let quad = Quad::new(
        NamedNode::new_unchecked("urn:s"),
        NamedNode::new_unchecked("urn:p"),
        Literal::from(true),
        NamedNode::new_unchecked("urn:g"),
    );
    transaction.insert(quad.as_ref());
    assert!(
        program
            .validate_readonly(&transaction, Options::default())
            .unwrap()
            .conforms
    );
    transaction.remove(quad.as_ref());
    let report = program
        .validate_readonly(&transaction, Options::default())
        .unwrap();
    assert!(!report.conforms);
    assert!(report.text.contains("MinCountConstraintComponent"));
    assert!(report.text.contains("SPARQLConstraintComponent"));
    drop(transaction);
    assert_eq!(store.len().unwrap(), 0);
}

#[test]
fn rule_validation_stages_additions_in_the_callers_transaction_until_commit() {
    let shapes = crate::IndexedShapesSource::parse_graph(r#"@prefix sh: <http://www.w3.org/ns/shacl#> .
      <urn:S> sh:targetNode <urn:s>;
        sh:rule [a sh:SPARQLRule; sh:construct "CONSTRUCT { $this <urn:derived> ?value } WHERE { $this <urn:p> ?value }"];
        sh:property [sh:path <urn:derived>; sh:hasValue true]."#, RdfFormat::Turtle, None).unwrap();
    let program = CompiledShapes::compile(shapes, true).unwrap();
    for commit in [false, true] {
        let store = Store::new().unwrap();
        let mut transaction = store.start_transaction().unwrap();
        let source = Quad::new(
            NamedNode::new_unchecked("urn:s"),
            NamedNode::new_unchecked("urn:p"),
            Literal::from(true),
            NamedNode::new_unchecked("urn:g"),
        );
        let derived = Quad::new(
            NamedNode::new_unchecked("urn:s"),
            NamedNode::new_unchecked("urn:derived"),
            Literal::from(true),
            GraphName::DefaultGraph,
        );
        transaction.insert(source.as_ref());
        assert!(
            program
                .validate_transaction(&mut transaction, Options::default())
                .unwrap()
                .conforms
        );
        assert!(transaction.contains(derived.as_ref()).unwrap());
        assert_eq!(store.len().unwrap(), 0);
        if commit {
            transaction.commit().unwrap();
        } else {
            drop(transaction);
        }
        assert_eq!(store.len().unwrap(), if commit { 2 } else { 0 });
    }
}

#[test]
fn staged_validation_retains_only_rule_additions_and_does_not_change_pending_input() {
    let shapes = crate::IndexedShapesSource::parse_graph(
        r#"@prefix sh: <http://www.w3.org/ns/shacl#> .
      <urn:S> sh:targetNode <urn:s>;
        sh:rule [a sh:TripleRule; sh:subject sh:this; sh:predicate <urn:derived>; sh:object true];
        sh:property [sh:path <urn:derived>; sh:hasValue true]."#,
        RdfFormat::Turtle,
        None,
    )
    .unwrap();
    let program = CompiledShapes::compile(shapes, true).unwrap();
    let store = Store::new().unwrap();
    let mut transaction = store.start_transaction().unwrap();
    let source = Quad::new(
        NamedNode::new_unchecked("urn:s"),
        NamedNode::new_unchecked("urn:p"),
        Literal::from(true),
        GraphName::DefaultGraph,
    );
    let derived = Quad::new(
        NamedNode::new_unchecked("urn:s"),
        NamedNode::new_unchecked("urn:derived"),
        Literal::from(true),
        GraphName::DefaultGraph,
    );
    transaction.insert(source.as_ref());
    assert!(
        program
            .validate_staged(&transaction, Options::default())
            .unwrap()
            .conforms
    );
    let outcome = program
        .validate_delta(&transaction, Options::default())
        .unwrap();
    assert!(outcome.report.conforms);
    assert_eq!(outcome.additions.len(), 1);

    assert!(transaction.contains(source.as_ref()).unwrap());
    assert!(!transaction.contains(derived.as_ref()).unwrap());
    assert_eq!(store.len().unwrap(), 0);
}

struct IndexedOnly<'source, G: GraphRead + ?Sized>(&'source G);

impl<'source, G: GraphRead + ?Sized> GraphRead for IndexedOnly<'source, G> {
    fn matching_quads<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a, 'source, G> {
        if subject.is_none() && predicate.is_none() && object.is_none() && graph.is_none() {
            return Box::new(std::iter::once(Err(Error::Runtime(
                "Full base enumeration forbidden".into(),
            )))) as Box<dyn Iterator<Item = _>>;
        }
        Box::new(self.0.matching_quads(subject, predicate, object, graph))
    }
    fn named_graphs(&self) -> impl Iterator<Item = Result<NamedOrBlankNode>> + '_ {
        self.0.named_graphs()
    }
    fn contains_named_graph(&self, graph: &NamedOrBlankNode) -> Result<bool> {
        self.0.contains_named_graph(graph)
    }
}

#[test]
fn staged_core_sparql_functions_and_reports_do_not_enumerate_the_base() {
    let shapes = crate::IndexedShapesSource::parse_graph(r#"@prefix sh: <http://www.w3.org/ns/shacl#> .
      <urn:has> a sh:SPARQLFunction; sh:ask "ASK { <urn:s> <urn:p> ?value }" .
      <urn:S> sh:targetNode <urn:s>;
        sh:rule [a sh:TripleRule; sh:subject sh:this; sh:predicate <urn:derived>; sh:object true];
        sh:property [sh:path <urn:derived>; sh:hasValue true];
        sh:sparql [sh:select "SELECT $this WHERE { GRAPH $shapesGraph { $currentShape <http://www.w3.org/ns/shacl#targetNode> $this } FILTER(!<urn:has>()) }"] ."#, RdfFormat::Turtle, None).unwrap();
    let program = CompiledShapes::compile(shapes, true).unwrap();
    let data = parse_dataset("<urn:g> {<urn:s> <urn:p> 1}", RdfFormat::TriG, None).unwrap();
    assert!(
        program
            .validate_staged(&IndexedOnly(&data), Options::default())
            .unwrap()
            .conforms
    );
    let empty = Dataset::new();
    let report = program
        .validate_staged(&IndexedOnly(&empty), Options::default())
        .unwrap();
    assert!(!report.conforms);
    assert!(report.text.contains("SPARQLConstraintComponent"));
}
