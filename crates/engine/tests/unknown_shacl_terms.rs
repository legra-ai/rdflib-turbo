//! Unknown `sh:` vocabulary is an annotation, exactly as in the reference
//! validator; only SHACL-JS, which the reference can execute, is refused.
use pyshacl_turbo_engine::oxigraph::io::RdfFormat;
use pyshacl_turbo_engine::oxigraph::model::vocab::rdf;
use pyshacl_turbo_engine::oxigraph::model::*;
use pyshacl_turbo_engine::{CompiledShapes, Error, IndexedShapesSource, Options};

const PREFIX: &str = "@prefix sh: <http://www.w3.org/ns/shacl#>.";

fn program(shapes: &str, advanced: bool) -> pyshacl_turbo_engine::Result<CompiledShapes> {
    let source =
        IndexedShapesSource::parse_graph(&format!("{PREFIX} {shapes}"), RdfFormat::Turtle, None)?;
    CompiledShapes::compile(source, advanced)
}

fn data() -> Dataset {
    let mut data = Dataset::new();
    data.insert(&Quad::new(
        NamedNode::new_unchecked("urn:a"),
        NamedNode::new_unchecked("urn:p"),
        Literal::from(1),
        GraphName::DefaultGraph,
    ));
    data
}

fn shacl(name: &str) -> Term {
    NamedNode::new_unchecked(format!("http://www.w3.org/ns/shacl#{name}")).into()
}

fn iri(iri: &str) -> Term {
    NamedNode::new_unchecked(iri).into()
}

/// The report fields that the reference comparison pins per result.
#[derive(Debug, PartialEq, Eq)]
struct ResultRow {
    focus: Term,
    component: Term,
    path: Term,
}

impl ResultRow {
    fn all(report: &Dataset) -> Vec<Self> {
        let result_type = shacl("ValidationResult");
        let mut rows: Vec<_> = report
            .iter()
            .filter(|q| q.predicate == rdf::TYPE && q.object == result_type.as_ref())
            .map(|q| Self::read(report, q.subject))
            .collect();
        rows.sort_by_key(|row| format!("{row:?}"));
        rows
    }

    fn read(report: &Dataset, result: NamedOrBlankNodeRef<'_>) -> Self {
        let object = |name: &str| -> Term {
            let predicate = NamedNode::new_unchecked(format!("http://www.w3.org/ns/shacl#{name}"));
            report
                .quads_for_subject(result)
                .find(|q| q.predicate == predicate.as_ref())
                .map(|q| q.object.into_owned())
                .expect(name)
        };
        Self {
            focus: object("focusNode"),
            component: object("sourceConstraintComponent"),
            path: object("resultPath"),
        }
    }
}

#[test]
fn unknown_predicates_and_types_validate_like_the_reference() {
    let shapes = "<urn:S> a sh:NodeShape, sh:FutureShape; sh:targetNode <urn:a>, <urn:b>;
        sh:futureAnnotation 'x';
        sh:property [ sh:path <urn:p>; sh:minCount 1; sh:futureConstraint 3;
                      a sh:FuturePropertyThing ].
        <urn:f> a sh:Function.";
    for advanced in [false, true] {
        let report = program(shapes, advanced)
            .unwrap()
            .validate(&mut data(), Options::default())
            .unwrap();
        // pyshacl 0.40.2: conforms False, one sh:MinCountConstraintComponent
        // result on focus <urn:b> with path <urn:p>.
        assert!(!report.conforms, "advanced={advanced}");
        assert_eq!(
            ResultRow::all(&report.graph),
            [ResultRow {
                focus: iri("urn:b"),
                component: shacl("MinCountConstraintComponent"),
                path: iri("urn:p"),
            }]
        );
    }
}

#[test]
fn shacl_javascript_is_refused_instead_of_ignored() {
    for shapes in [
        "<urn:S> sh:targetNode <urn:b>; sh:js [ sh:jsFunctionName 'f' ].",
        "<urn:S> sh:targetNode <urn:b>; sh:sparql [ a sh:JSConstraint ].",
        "<urn:f> a sh:JSFunction; sh:jsLibrary [ sh:jsLibraryURL 'x' ].",
        "<urn:S> sh:targetNode <urn:b>. <urn:T> a sh:JSTargetType.",
    ] {
        for advanced in [false, true] {
            assert!(
                matches!(program(shapes, advanced), Err(Error::Unsupported(_))),
                "{shapes} advanced={advanced}"
            );
        }
    }
}
