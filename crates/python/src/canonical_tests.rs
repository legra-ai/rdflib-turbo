use std::ffi::CStr;

use pyo3::types::PyModule;
use pyshacl_turbo_engine::GraphRead;

use super::*;

fn execute(source: &CStr) {
    Python::initialize();
    Python::attach(|py| {
        let module = PyModule::new(py, "typed_shapes_tests").unwrap();
        crate::register(&module, crate::provider_tests::test_regex_policy()).unwrap();
        py.run(source, Some(&module.dict()), None).unwrap();
    });
}

#[test]
fn actual_lru_compares_complete_inputs_despite_forced_hash_collision() {
    Python::initialize();
    Python::attach(|py| {
        let module = PyModule::new(py, "shape_collision_tests").unwrap();
        crate::register(&module, crate::provider_tests::test_regex_policy()).unwrap();
        for (name, value) in [
            ("first", "urn:a"),
            ("different", "urn:b"),
            ("equal", "urn:a"),
        ] {
            let mut builder = arena::Builder::default();
            builder.push_quad(GeneralizedQuadRef {
                subject: NamedNodeRef::new_unchecked("urn:S").into(),
                predicate: NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#targetNode")
                    .into(),
                object: NamedNodeRef::new_unchecked(value).into(),
                graph_name: GraphNameRef::DefaultGraph,
            });
            let mut key = ShapesInput::from_records(builder.finish(), false);
            key.hash = 7;
            module.setattr(name, Py::new(py, key).unwrap()).unwrap();
        }
        py.run(
            c"from functools import lru_cache
assert hash(first) == hash(different) == hash(equal) == 7
assert first == equal and first != different and first != 'unrelated'
@lru_cache(maxsize=128)
def compile(key):
    return CompiledShapes.from_input(key, False, True, [], _policy=_REGEX_POLICY)
a, b = compile(first), compile(different)
assert a is not b and compile(equal) is a
assert compile.cache_info().currsize == 2
try:
    first.hash = 1
except AttributeError:
    pass
else:
    raise AssertionError('mutable key')
try:
    first < different
except TypeError:
    pass
else:
    raise AssertionError('unordered key accepted comparison')",
            Some(&module.dict()),
            None,
        )
        .unwrap();
    });
}

#[test]
fn programmatic_terms_keep_full_positions_and_identifier_lexicals() {
    execute(c"iri = lambda value: (0, value, None, None)
blank = lambda value: (1, value, None, None)
literal = lambda value, datatype=None, language=None: (2, value, datatype, language)
target = iri('http://www.w3.org/ns/shacl#targetNode')
for shape in [iri('relative-shape'), blank(''), blank('not a standard blank label'), literal('shape')]:
    key = ShapesInput(iter([([shape, target, iri('urn:s')], [(0, 1, 2, None)])]))
    assert len(CompiledShapes.from_input(key, False, True, [], _policy=_REGEX_POLICY).shape_nodes()) == 1
terms = [iri('urn:s'), blank('predicate'), literal('01', 'urn:exact'), literal('metadata')]
first = ShapesInput(iter([(terms, [(3, 1, 2, None)])]))
second = ShapesInput(iter([(terms, [(3, 1, 2, None), (3, 1, 2, None)])]))
assert first == second and hash(first) == hash(second)
assert len(CompiledShapes.from_input(first, False, True, [], _policy=_REGEX_POLICY).shape_nodes()) == 0
for left, right in [(literal('value'), literal('value', 'http://www.w3.org/2001/XMLSchema#string')), (literal('01', 'urn:integer'), literal('1', 'urn:integer'))]:
    assert ShapesInput(iter([([blank('s'), iri('urn:p'), left], [(0, 1, 2, None)])])) != ShapesInput(iter([([blank('s'), iri('urn:p'), right], [(0, 1, 2, None)])]))
assert ShapesInput(iter([([blank('s'), iri('urn:p'), literal('bonjour', language='FR')], [(0, 1, 2, None)])])) == ShapesInput(iter([([blank('s'), iri('urn:p'), literal('bonjour', language='fr')], [(0, 1, 2, None)])]))
");
}

#[test]
fn dataset_keys_keep_contexts_and_declarations_and_deduplicate_implicit_names() {
    execute(c"iri = lambda value: (0, value, None, None)
records = [iri('urn:S'), iri('http://www.w3.org/ns/shacl#targetNode'), iri('urn:s'), iri('urn:a'), iri('urn:b')]
a = ShapesInput(iter([(records, [(0, 1, 2, 3)])]), True)
b = ShapesInput(iter([(records, [(0, 1, 2, 4)])]), True)
assert a != b
assert a == ShapesInput(iter([(records, [(0, 1, 2, 3)])]), True, iter([iri('urn:a')]))
assert a != ShapesInput(iter([(records, [(0, 1, 2, 3)])]), True, iter([iri('urn:empty')]))
assert ShapesInput(iter([])) != ShapesInput(iter([]), True)
assert len(CompiledShapes.from_input(a, False, True, [], _policy=_REGEX_POLICY).shape_nodes()) == 1
");
    let mut builder = arena::Builder::default();
    builder.push_graph(NamedNodeRef::new_unchecked("urn:empty").into());
    let source = ShapesInput::from_records(builder.finish(), true)
        .source()
        .unwrap();
    assert_eq!(source.matching_quads(None, None, None, None).count(), 0);
    assert!(
        source
            .contains_named_graph(&NamedNode::new("urn:empty").unwrap().into())
            .unwrap()
    );
}

#[test]
fn every_late_record_is_validated_before_cache_lookup() {
    execute(c"from functools import lru_cache
records = [(0, 'urn:S', None, None), (0, 'http://www.w3.org/ns/shacl#targetNode', None, None), (0, 'urn:s', None, None)]
valid = (records, [(0, 1, 2, None)])
@lru_cache(maxsize=128)
def compile(key):
    return CompiledShapes.from_input(key, False, True, [], _policy=_REGEX_POLICY)
def late():
    yield valid
    raise RuntimeError('late iterator failure')
def invalid_late():
    yield valid
    yield ([], [(0, 0, 0, None)])
for source, error in [(late(), RuntimeError), (invalid_late(), ValueError), (iter([([(99, 'invalid', None, None)], [])]), ValueError), (iter([(records, [(0, 1, 2, 99)])]), ValueError), (iter([(records + [(2, 'literal', None, None)], [(0, 1, 2, 3)])]), ValueError)]:
    try:
        compile(ShapesInput(source))
    except error:
        pass
    else:
        raise AssertionError('invalid input accepted')
    assert compile.cache_info().currsize == 0
class Records(list):
    def __iter__(self):
        raise AssertionError('oversized batch extracted records')
for batch in [(Records(records), [(0, 1, 2, None)] * 257), (Records(records * 342), [])]:
    try:
        ShapesInput(iter([batch]))
    except ValueError as cause:
        assert '256' in str(cause)
    else:
        raise AssertionError('oversized batch accepted')
assert len(compile(ShapesInput(iter([valid]))).shape_nodes()) == 1
");
}

#[test]
fn inputs_drop_python_owners_and_eviction_keeps_programs_alive() {
    execute(c"import gc
import weakref
from functools import lru_cache
class Text(str):
    pass
class Batch(list):
    pass
text = Text('urn:s')
records = [(0, 'urn:S', None, None), (0, 'http://www.w3.org/ns/shacl#targetNode', None, None), (0, text, None, None)]
source = Batch([(records, [(0, 1, 2, None)])])
text_ref, source_ref = weakref.ref(text), weakref.ref(source)
key = ShapesInput(iter(source))
del text, source, records
gc.collect()
assert text_ref() is None and source_ref() is None
@lru_cache(maxsize=128)
def compile(key):
    return CompiledShapes.from_input(key, False, True, [], _policy=_REGEX_POLICY)
original = compile(key)
for index in range(128):
    compile(ShapesInput(iter([([(0, 'urn:shape' + str(index), None, None), (0, 'http://www.w3.org/ns/shacl#targetNode', None, None), (0, 'urn:s', None, None)], [(0, 1, 2, None)])])))
assert compile.cache_info().currsize == 128
assert compile(key) is not original
compile.cache_clear()
del key
gc.collect()
assert original.shape_nodes() == ['<urn:S>']
");
}
