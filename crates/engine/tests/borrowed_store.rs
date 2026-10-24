use pyshacl_turbo_engine::oxigraph::io::RdfFormat;
use pyshacl_turbo_engine::oxigraph::model::*;
use pyshacl_turbo_engine::{CompiledShapes, Error, GraphRead, Options, Result, parse_graph};

struct IndexedOnly<'a>(&'a Dataset);

impl<'source> GraphRead for IndexedOnly<'source> {
    fn matching_quads<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a, 'source> {
        assert!(
            subject.is_some() || predicate.is_some() || object.is_some(),
            "SHACL attempted to scan the complete base relation"
        );
        self.0.matching_quads(subject, predicate, object, graph)
    }

    fn named_graphs(&self) -> impl Iterator<Item = Result<NamedOrBlankNode>> + '_ {
        self.0.named_graphs()
    }

    fn contains_named_graph(&self, graph: &NamedOrBlankNode) -> Result<bool> {
        self.0.contains_named_graph(graph)
    }
}

#[test]
fn fixed_targets_rules_and_validation_need_no_whole_source_scan_or_copy() {
    let shapes = pyshacl_turbo_engine::IndexedShapesSource::parse_graph("@prefix sh: <http://www.w3.org/ns/shacl#>. <urn:S> sh:targetNode <urn:s>; sh:rule [a sh:SPARQLRule; sh:construct 'CONSTRUCT { $this <urn:derived> ?value } WHERE { $this <urn:value> ?value }']; sh:property [sh:path <urn:derived>; sh:hasValue 2].", RdfFormat::Turtle, None).unwrap();
    let program = CompiledShapes::compile(shapes, true).unwrap();
    let data = parse_graph(
        "<urn:s> <urn:value> 2. <urn:unrelated> <urn:p> <urn:o>.",
        RdfFormat::Turtle,
        None,
    )
    .unwrap();
    let source = IndexedOnly(&data);
    let staged = program.validate_delta(&source, Options::default()).unwrap();
    assert!(staged.report.conforms);
    assert_eq!(staged.additions.len(), 1);
    assert_eq!(data.len(), 2);
    assert!(!data.contains(QuadRef::new(
        NamedNodeRef::new("urn:s").unwrap(),
        NamedNodeRef::new("urn:derived").unwrap(),
        LiteralRef::new_typed_literal(
            "2",
            NamedNodeRef::new("http://www.w3.org/2001/XMLSchema#integer").unwrap()
        ),
        GraphNameRef::DefaultGraph
    )));
}

struct FailingDescription<'a>(&'a Dataset);

impl<'source> GraphRead for FailingDescription<'source> {
    fn matching_quads<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a, 'source> {
        let matches: Box<dyn Iterator<Item = Result<GeneralizedQuad>> + 'a> =
            if matches!(subject, Some(TermRef::BlankNode(_))) {
                Box::new(std::iter::once(Err(Error::Runtime(
                    "injected blank description read failure".into(),
                ))))
            } else {
                Box::new(self.0.matching_quads(subject, predicate, object, graph))
            };
        matches
    }

    fn named_graphs(&self) -> impl Iterator<Item = Result<NamedOrBlankNode>> + '_ {
        self.0.named_graphs()
    }

    fn contains_named_graph(&self, graph: &NamedOrBlankNode) -> Result<bool> {
        self.0.contains_named_graph(graph)
    }
}

#[test]
fn report_description_storage_errors_are_execution_errors_not_partial_reports() {
    let shapes = pyshacl_turbo_engine::IndexedShapesSource::parse_graph("@prefix sh: <http://www.w3.org/ns/shacl#>. <urn:S> sh:targetNode <urn:s>; sh:property [sh:path <urn:p>; sh:nodeKind sh:IRI].", RdfFormat::Turtle, None).unwrap();
    let program = CompiledShapes::compile(shapes, false).unwrap();
    let data = parse_graph(
        "<urn:s> <urn:p> _:value. _:value <urn:label> 'description'.",
        RdfFormat::Turtle,
        None,
    )
    .unwrap();
    let error = program
        .validate_readonly(&FailingDescription(&data), Options::default())
        .err()
        .unwrap();
    assert!(
        error
            .to_string()
            .contains("injected blank description read failure")
    );
    assert_eq!(data.len(), 2);
}
