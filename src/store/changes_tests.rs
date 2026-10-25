//! Private inference changes preserve atomic publication and owning cursors.
use pyo3::prelude::*;
use pyo3::types::PyModule;

use super::term_output::NativeTermFactories;

fn fixture(py: Python<'_>) -> Bound<'_, PyModule> {
    let module = super::import_tests::fixture(py);
    module.add_class::<NativeTermFactories>().unwrap();
    py.run(
        c"
def restore(value, datatype, language):
    return Literal(value, datatype=None if datatype is None else URIRef(datatype), lang=language)
factories = NativeTermFactories(URIRef, BNode, restore, str.__new__, True)
def collect(changes, method):
    rows = []
    while True:
        batch, done = getattr(changes, method)()
        rows.extend(batch)
        if done: return rows
def key(row):
    return tuple(None if t is None else (type(t).__name__, str(t),
        None if not isinstance(t, Literal) or t.datatype is None else str(t.datatype),
        t.language if isinstance(t, Literal) else None) for t in row)
",
        Some(&module.dict()),
        None,
    )
    .unwrap();
    module
}

#[test]
fn private_changes_keep_full_term_removals_and_survive_owner_close() {
    Python::initialize();
    Python::attach(|py| {
        let module = fixture(py);
        py.run(
            c"
statement = (URIRef('urn:s'), BNode('predicate'), URIRef('urn:o'), None)
source = NativeStore.from_records([statement], (), inputs)
changes = source._infer_changes('owlrl', (None, ''), factories)
source.close()
removed = collect(changes, 'next_removals')
added = collect(changes, 'next_additions')
assert removed == [statement]
assert tuple(type(t) for t in removed[0]) == (URIRef, BNode, URIRef, type(None))
assert added and all(row[3] is None for row in added)
assert changes.outcome.error_messages == []
changes.close()
changes.close()
try:
    changes.next_additions()
except RuntimeError as error:
    assert 'closed' in str(error)
else:
    raise AssertionError('closed changes remained readable')
",
            Some(&module.dict()),
            None,
        )
        .unwrap();
    });
}

#[test]
fn private_changes_close_both_leases_on_reentrant_factory_error() {
    Python::initialize();
    Python::attach(|py| {
        let module = fixture(py);
        py.run(c"
source = NativeStore.from_records([(URIRef('urn:s'), URIRef('urn:p'), Literal('value'), None)], (), inputs)
failure = ValueError('synthetic restoration failure')
def broken(value, datatype, language):
    changes.close()
    raise failure
bad_factories = NativeTermFactories(URIRef, BNode, broken, str.__new__)
changes = source._infer_changes('owlrl', (None, ''), bad_factories)
source.close()
try:
    collect(changes, 'next_additions')
except ValueError as error:
    assert error is failure
else:
    raise AssertionError('factory failure did not propagate')
try:
    changes.next_removals()
except RuntimeError as error:
    assert 'closed' in str(error)
else:
    raise AssertionError('the other change lease survived failure')
", Some(&module.dict()), None).unwrap();
    });
}

#[test]
fn private_capture_late_cycle_aborts_derived_additions_and_cleanup() {
    Python::initialize();
    Python::attach(|py| {
        let module = fixture(py);
        py.run(
            c"
rdf = 'http://www.w3.org/1999/02/22-rdf-syntax-ns#'
owl = 'http://www.w3.org/2002/07/owl#'
rows = [
    (URIRef('urn:C'), URIRef(owl+'hasKey'), BNode('list'), None),
    (BNode('list'), URIRef(rdf+'first'), URIRef('urn:key'), None),
    (BNode('list'), URIRef(rdf+'rest'), BNode('list'), None),
    (URIRef('urn:a'), URIRef(rdf+'type'), URIRef('urn:C'), None),
    (URIRef('urn:b'), URIRef(rdf+'type'), URIRef('urn:C'), None),
    (URIRef('urn:s'), BNode('predicate'), URIRef('urn:o'), None),
]
source = NativeStore.from_records(rows, [URIRef('urn:empty')], inputs)
before = set(map(str, json.loads(source.match_quads('[null,null,null,null]'))))
domains = source.named_graphs()
try:
    source._infer_changes('both', (None, ''), factories, options=(True, True))
except ValueError as error:
    assert str(error) == 'List contains a recursive rdf:rest reference'
else:
    raise AssertionError('recursive list did not fail')
assert set(map(str, json.loads(source.match_quads('[null,null,null,null]')))) == before
assert source.named_graphs() == domains
source.close()
",
            Some(&module.dict()),
            None,
        )
        .unwrap();
    });
}

#[test]
fn captured_scopes_remove_only_exact_source_and_destination_twins() {
    Python::initialize();
    Python::attach(|py| {
        let module = fixture(py);
        py.run(c"
a, b, c = URIRef('urn:scope'), BNode('urn:scope'), URIRef('urn:third')
triple = (Literal('subject'), BNode('predicate'), URIRef('urn:o'))
unrelated = (URIRef('urn:other'), triple[1], URIRef('urn:other-object'), b)
records = [(*triple, scope) for scope in (a,b,c)] + [unrelated]
source = NativeStore.from_records(records, [URIRef('urn:empty')], inputs)
scope = lambda kind: json.dumps({'type':kind, 'value':'urn:scope'})
changes = source._infer_changes('owlrl', (scope('uri'), scope('bnode')), factories)
removed = collect(changes, 'next_removals')
added = collect(changes, 'next_additions')
assert set(map(key, removed)) == {key((*triple, a)), key((*triple, b))}
assert added and all(type(row[3]) is BNode and row[3] == b for row in added)
actual = json.loads(source.match_quads('[null,null,null,null]'))
assert any(row[0] == {'type':'literal','value':'subject'} and row[3] == {'type':'uri','value':'urn:third'} for row in actual)
assert any(row[0] == {'type':'uri','value':'urn:other'} and row[3] == {'type':'bnode','value':'urn:scope'} for row in actual)
assert '<urn:empty>' in json.loads(source.named_graphs())
changes.close()
source.close()
", Some(&module.dict()), None).unwrap();
    });
}

#[test]
fn stable_closure_changes_finish_empty_and_keep_existing_rows() {
    Python::initialize();
    Python::attach(|py| {
        let module = fixture(py);
        py.run(
            c"
statement = (URIRef('urn:s'), URIRef('urn:p'), URIRef('urn:o'), None)
initial = NativeStore.from_records([statement], (), inputs)
first = initial._infer_changes('owlrl', (None, ''), factories)
# bounded: this fixture has one input statement and its fixed OWL closure.
records = [statement] + collect(first, 'next_additions')
assert collect(first, 'next_removals') == []
first.close()
initial.close()
source = NativeStore.from_records(records, (), inputs)
before = source.match_quads('[null,null,null,null]')
changes = source._infer_changes('owlrl', (None, ''), factories)
assert changes.next_additions() == ([], True)
assert changes.next_removals() == ([], True)
assert source.match_quads('[null,null,null,null]') == before
changes.close()
source.close()
",
            Some(&module.dict()),
            None,
        )
        .unwrap();
    });
}
