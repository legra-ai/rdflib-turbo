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
        BooleanInput::new(py, graph, &PyDict::new(py), false, classes, None).unwrap(),
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
            data: &input.data,
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
