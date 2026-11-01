use super::*;
use caller::Caller;
use pyshacl_turbo_engine::{GraphRead, SelectedValueFrame, SelectedValueSource};

fn input<'py>(py: Python<'py>, code: &std::ffi::CStr) -> (BooleanInput, Bound<'py, PyDict>) {
    let scope = PyDict::new(py);
    py.run(code, Some(&scope), None).unwrap();
    let classes = ["URIRef", "BNode", "Literal"].map(|name| {
        scope
            .get_item(name)
            .unwrap()
            .unwrap()
            .cast_into::<PyType>()
            .unwrap()
            .unbind()
    });
    let graph = scope.get_item("graph").unwrap().unwrap().unbind();
    scope.del_item("graph").unwrap();
    (
        BooleanInput::new(py, graph, &PyDict::new(py), false, classes, None, None).unwrap(),
        scope,
    )
}
fn caller(py: Python<'_>, input: &BooleanInput) -> Arc<Caller> {
    Caller::new(
        py,
        input,
        crate::provider_tests::test_comparison_host()
            .capture_source(py)
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn compiled_literal_operands_preserve_their_native_lexical_domain() {
    Python::initialize();
    Python::attach(|py| {
        let (input, _) = input(
            py,
            c"from rdflib import Graph,URIRef,BNode,Literal\ngraph=Graph()",
        );
        let factories = source::Factories::bind(py, &input.classes);
        for literal in [
            Literal::new_simple_literal("String"),
            Literal::new_language_tagged_literal("word", "en").unwrap(),
            Literal::new_typed_literal(
                "01",
                NamedNode::new_unchecked("http://www.w3.org/2001/XMLSchema#integer"),
            ),
        ] {
            let object = factories.python_term(literal.as_ref().into()).unwrap();
            assert_eq!(object.str().unwrap().to_str().unwrap(), literal.value());
            assert_eq!(
                object
                    .getattr("language")
                    .unwrap()
                    .extract::<Option<String>>()
                    .unwrap()
                    .as_deref(),
                literal.language()
            );
            if literal.value() == "01" {
                assert_eq!(
                    object.getattr("value").unwrap().extract::<i64>().unwrap(),
                    1
                );
            }
        }
    });
}

#[test]
fn legacy_conjunctive_input_keeps_default_and_named_graphs_distinct() {
    Python::initialize();
    Python::attach(|py| {
        let (input, _) = input(
            py,
            c"from rdflib import ConjunctiveGraph,URIRef,BNode,Literal
graph=ConjunctiveGraph()
graph.default_context.add((URIRef('urn:s'),URIRef('urn:default'),Literal('value')))
graph.get_context(URIRef('urn:named')).add((URIRef('urn:s'),URIRef('urn:named-p'),Literal('value')))",
        );
        let data = input.data.unwrap();
        assert_eq!(data.len(), 2);
        assert!(
            data.iter()
                .any(|q| q.predicate.as_str() == "urn:default" && q.graph_name.is_default_graph())
        );
        assert!(data.iter().any(|q| q.predicate.as_str() == "urn:named-p"
            && q.graph_name == GraphNameRef::NamedNode(NamedNodeRef::new_unchecked("urn:named"))));
    });
}
#[test]
fn selected_membership_is_not_required_to_equal_raw_membership() {
    Python::initialize();
    Python::attach(|py| {
        let (input, _) = input(
            py,
            c"from rdflib import Graph,URIRef,BNode,Literal
class Changed(Graph):
 def __iter__(self): return iter(((URIRef('urn:s'),URIRef('urn:p'),Literal(False)),))
 def triples(self,pattern): return iter(())
graph=Changed()",
        );
        let caller = caller(py, &input);
        let frame = caller.begin_frame().unwrap();
        assert!(
            frame
                .predicate(
                    &NamedNode::new("urn:s").unwrap().into(),
                    &NamedNode::new("urn:p").unwrap(),
                    None
                )
                .unwrap()
                .is_empty()
        );
        frame.close();
    });
}
#[test]
fn foreign_ended_and_wrong_thread_slots_fail_before_python_access() {
    Python::initialize();
    Python::attach(|py| {
        let (input, _) = input(
            py,
            c"from rdflib import Graph,URIRef,BNode,Literal
graph=Graph();graph.add((URIRef('urn:s'),URIRef('urn:p'),Literal(False)))",
        );
        let caller = caller(py, &input);
        let frame = caller.begin_frame().unwrap();
        let rows = frame
            .predicate(
                &NamedNode::new("urn:s").unwrap().into(),
                &NamedNode::new("urn:p").unwrap(),
                None,
            )
            .unwrap();
        let slot = rows[0].slot.unwrap();
        assert!(frame.retain(slot + 1).is_err());
        let foreign = frame::Frame {
            caller: Arc::downgrade(&caller),
            id: 1,
            issuer: Arc::new(()),
        };
        assert!(foreign.retain(slot).is_err());
        let other = frame.clone();
        assert!(
            std::thread::spawn(move || other.retain(slot))
                .join()
                .unwrap()
                .is_err()
        );
        frame.close();
        assert!(frame.retain(slot).is_err());
    });
}
#[test]
fn acquiring_and_promoting_report_owners_do_not_read_cache_properties() {
    Python::initialize();
    Python::attach(|py| {
        let (input,scope)=input(py,c"from rdflib import Graph,URIRef,BNode,Literal
from rdflib.namespace import XSD
events=[]
class Observed(Literal):
 @property
 def value(self): events.append('value'); raise AssertionError('unexpected getter')
 @property
 def ill_typed(self): events.append('ill'); return True
graph=Graph();graph.add((URIRef('urn:s'),URIRef('urn:p'),Observed('false',datatype=XSD.boolean,normalize=False)))");
        assert!(
            scope
                .get_item("events")
                .unwrap()
                .unwrap()
                .extract::<Vec<String>>()
                .unwrap()
                .is_empty()
        );
        let caller = caller(py, &input);
        let frame = caller.begin_frame().unwrap();
        let rows = frame
            .predicate(
                &NamedNode::new("urn:s").unwrap().into(),
                &NamedNode::new("urn:p").unwrap(),
                None,
            )
            .unwrap();
        let slot = rows[0].slot.unwrap();
        let owner = frame.retain(slot).unwrap();
        assert!(
            scope
                .get_item("events")
                .unwrap()
                .unwrap()
                .extract::<Vec<String>>()
                .unwrap()
                .is_empty()
        );
        assert!(
            !frame
                .datatype(slot, &NamedNode::new(source::BOOLEAN).unwrap())
                .unwrap()
        );
        assert_eq!(
            scope
                .get_item("events")
                .unwrap()
                .unwrap()
                .extract::<Vec<String>>()
                .unwrap(),
            vec!["ill"]
        );
        frame.close();
        drop(caller);
        drop(input);
        assert!(!owner.0.as_any().downcast_ref::<live::Owner>().unwrap().word);
    });
}
#[test]
fn no_canonical_candidate_has_no_source_capability() {
    Python::initialize();
    Python::attach(|py| {
        let (input, _) = input(
            py,
            c"from rdflib import Graph,URIRef,BNode,Literal
graph=Graph();graph.add((URIRef('urn:s'),URIRef('urn:p'),Literal('plain')))",
        );
        assert!(!input.canonical_booleans);
        let captured = selection::Captured {
            data: input.data.as_deref().map(|data| &**data),
            caller: None,
        };
        assert!(captured.selected_value_source().is_none());
    });
}
#[test]
fn current_boolean_carrier_layouts_are_explicit() {
    println!(
        "DEMAND_LAYOUT input={} captured={} frame={} owner={} acquired={} slot={} observation={} selected={}",
        std::mem::size_of::<BooleanInput>(),
        std::mem::size_of::<selection::Captured<'_>>(),
        std::mem::size_of::<frame::Frame>(),
        std::mem::size_of::<live::Owner>(),
        std::mem::size_of::<live::Acquired>(),
        std::mem::size_of::<pyshacl_turbo_engine::SelectedSlot>(),
        std::mem::size_of::<pyshacl_turbo_engine::BooleanObservation>(),
        std::mem::size_of::<pyshacl_turbo_engine::SelectedLiteral>()
    );
    let data = Dataset::new();
    let rows = super::borrowed::rows(Some(&data), None, None, None, None, None);
    println!(
        "BORROWED_LAYOUT cursor={} rows={} compiled={} pycompiled={}",
        std::mem::size_of::<super::borrowed::Cursor>(),
        std::mem::size_of_val(&rows),
        std::mem::size_of::<pyshacl_turbo_engine::CompiledShapes>(),
        std::mem::size_of::<crate::PyCompiledShapes>(),
    );
}

#[test]
fn borrowed_cursor_delegates_early_close_to_actual_source_view() {
    Python::initialize();
    Python::attach(|py| {
        let scope = PyDict::new(py);
        py.run(
            c"from rdflib import Graph, URIRef, BNode, Literal
from pyshacl.graph_abstraction import DataGraph
events=[]
class Rows:
 def __init__(self,rows): self.rows=rows
 def __iter__(self): return self
 def __next__(self): return next(self.rows)
 def close(self): events.append('close'); self.rows.close()
class Source(Graph):
 def triples(self,pattern): return Rows(super().triples(pattern))
graph=Source()
graph.add((URIRef('urn:s'),URIRef('urn:p'),Literal('one')))
graph.add((URIRef('urn:s'),URIRef('urn:p'),Literal('two')))
view=DataGraph.from_rdflib_graph(graph)
rows=view.triples((None,None,None)); next(rows); del rows
assert events==['close']
events.clear()
",
            Some(&scope),
            None,
        )
        .unwrap();
        let classes = ["URIRef", "BNode", "Literal"].map(|name| {
            scope
                .get_item(name)
                .unwrap()
                .unwrap()
                .cast_into::<PyType>()
                .unwrap()
                .unbind()
        });
        let graph = scope.get_item("graph").unwrap().unwrap().unbind();
        let view = scope.get_item("view").unwrap().unwrap().unbind();
        let input = BooleanInput::new(
            py,
            graph,
            &PyDict::new(py),
            false,
            classes,
            None,
            Some(view),
        )
        .unwrap();
        let caller = caller(py, &input);
        let mut rows = super::borrowed::rows(None, Some(&caller), None, None, None, None);
        assert!(rows.next().unwrap().is_ok());
        drop(rows);
        caller.finish(py, Ok(())).unwrap();
        py.run(c"assert events==['close'], events", Some(&scope), None)
            .unwrap();
    });
}

#[test]
fn datatype_uses_original_getattr_default_only_for_missing_assessment() {
    Python::initialize();
    Python::attach(|py| {
        let (input, _) = input(
            py,
            c"from rdflib import Graph,URIRef,BNode,Literal
from rdflib.namespace import XSD
class Missing(Literal):
 @property
 def ill_typed(self): raise AttributeError('assessment not exposed')
value=Missing('false',datatype=XSD.boolean,normalize=False)
assert getattr(value,'ill_typed',None) is None and value.value is False
graph=Graph();graph.add((URIRef('urn:s'),URIRef('urn:p'),value))",
        );
        let caller = caller(py, &input);
        let frame = caller.begin_frame().unwrap();
        let rows = frame
            .predicate(
                &NamedNode::new("urn:s").unwrap().into(),
                &NamedNode::new("urn:p").unwrap(),
                None,
            )
            .unwrap();
        assert!(
            frame
                .datatype(
                    rows[0].slot.unwrap(),
                    &NamedNode::new(source::BOOLEAN).unwrap()
                )
                .unwrap()
        );
        frame.close();
    });
}

#[test]
fn selected_focus_existing_caller_layouts_are_observed() {
    println!(
        "FOCUS_CALLER_LAYOUT Caller={} ActiveFrame={} Frame={} Acquired={} Owner={}",
        std::mem::size_of::<super::caller::Caller>(),
        std::mem::size_of::<super::caller::ActiveFrame>(),
        std::mem::size_of::<super::frame::Frame>(),
        std::mem::size_of::<super::live::Acquired>(),
        std::mem::size_of::<super::live::Owner>()
    );
    assert_eq!(std::mem::size_of::<super::live::Acquired>(), 24);
}
