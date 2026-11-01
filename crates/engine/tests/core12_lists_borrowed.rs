//! Lazy lists exercise indexed traversal without constructing their RDF graph.

use pyshacl_turbo_engine::oxigraph::{io::RdfFormat, model::*};
use pyshacl_turbo_engine::{
    CompiledShapes, Error, GraphRead, IndexedShapesSource, Options, Result,
};

struct LazyList {
    length: usize,
    fail: bool,
}

impl LazyList {
    fn edge(
        &self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
    ) -> Result<Option<GeneralizedQuad>> {
        let (Some(TermRef::NamedNode(subject)), Some(TermRef::NamedNode(predicate))) =
            (subject, predicate)
        else {
            return Err(Error::Runtime("unindexed list read".into()));
        };
        let Some(index) = subject
            .as_str()
            .strip_prefix("urn:list:")
            .and_then(|n| n.parse::<usize>().ok())
        else {
            return Ok(None);
        };
        if index >= self.length {
            return Ok(None);
        }
        if self.fail && index == 3 {
            return Err(Error::Runtime("injected list read failure".into()));
        }
        let object: Term = match predicate.as_str() {
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#first" => {
                Literal::from(index as u64).into()
            }
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest" => {
                NamedNode::new(if index + 1 == self.length {
                    "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil".to_owned()
                } else {
                    format!("urn:list:{}", index + 1)
                })
                .unwrap()
                .into()
            }
            _ => return Ok(None),
        };
        Ok(Some(GeneralizedQuad::new(
            subject,
            predicate,
            object,
            GraphName::DefaultGraph,
        )))
    }
}

impl GraphRead for LazyList {
    fn selected_focus_source(&self) -> Option<&dyn pyshacl_turbo_engine::SelectedFocusSource> {
        None
    }
    fn selected_value_source(&self) -> Option<&dyn pyshacl_turbo_engine::SelectedValueSource> {
        None
    }

    fn boolean_object_state(
        &self,
        _: &pyshacl_turbo_engine::oxigraph::model::GeneralizedQuad,
    ) -> pyshacl_turbo_engine::Result<Option<pyshacl_turbo_engine::BuiltinBooleanState>> {
        Ok(None)
    }

    fn matching_quads<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
        graph: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a> {
        let edge = self.edge(subject, predicate).map(|edge| {
            edge.filter(|q| {
                object.is_none_or(|o| o == q.object.as_ref())
                    && graph.is_none_or(|g| g == GraphNameRef::DefaultGraph)
            })
        });
        std::iter::once(edge).filter_map(|edge| edge.transpose())
    }

    fn named_graphs(&self) -> impl Iterator<Item = Result<NamedOrBlankNode>> + '_ {
        std::iter::empty()
    }
    fn contains_named_graph(&self, _: &NamedOrBlankNode) -> Result<bool> {
        Ok(false)
    }
}

fn program(count: usize) -> CompiledShapes {
    CompiledShapes::compile(IndexedShapesSource::parse_graph(&format!(
        "@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:S> sh:targetNode <urn:list:0>; sh:minListLength {count}."
    ), RdfFormat::Turtle, None).unwrap(), false).unwrap()
}

#[test]
fn lazy_list_checks_ten_thousand_cells_without_enumerating_the_source() {
    let source = LazyList {
        length: 10_000,
        fail: false,
    };
    assert!(
        program(10_000)
            .validate_staged(&source, Options::default())
            .unwrap()
            .conforms
    );
    let report = program(10_001)
        .validate_staged(&source, Options::default())
        .unwrap();
    assert!(!report.conforms);
    assert!(report.graph.len() < 30, "reports must not copy list cells");
}

#[test]
fn list_storage_failures_remain_execution_errors() {
    let source = LazyList {
        length: 10,
        fail: true,
    };
    let result = program(0).validate_staged(&source, Options::default());
    assert!(
        matches!(result, Err(Error::Runtime(message)) if message.contains("injected list read failure"))
    );
}
