//! Dataset reads keep physical contexts and a bounded owning continuation.
use super::term_output::NativeTermFactories;
use pyo3::prelude::*;

#[test]
fn full_quad_reads_preserve_scope_domains_and_owner_lifetime() {
    Python::initialize();
    Python::attach(|py| {
        let module = super::import_tests::fixture(py);
        module.add_class::<NativeTermFactories>().unwrap();
        py.run(
            c"
def restore(value, datatype, language):
    return Literal(value, datatype=None if datatype is None else URIRef(datatype), lang=language)
factories = NativeTermFactories(URIRef, BNode, restore, str.__new__, True)
def encode(value):
    return ('uri' if isinstance(value, URIRef) else 'bnode', str(value), None, None)
def collect(first):
    rows, cursor = first
    assert len(rows) <= 128
    if cursor is not None:
        try:
            while True:
                batch, done = cursor.next_batch()
                assert len(batch) <= 128
                rows.extend(batch)
                if done: break
        finally: cursor.close()
    return rows
graphs = [None, URIRef('urn:scope'), BNode('urn:scope')]
records = [(Literal('subject', lang='en'), BNode('predicate'), Literal(str(i)), g)
           for i in range(150) for g in graphs]
source = NativeStore.from_records(records, (), inputs)
pattern = (None, None, None)
for graph in graphs:
    scope = ('default', None) if graph is None else ('graph', encode(graph))
    rows = collect(source.read_quads(pattern, scope, factories))
    assert len(rows) == 150
    assert all(type(row[3]) is type(graph) and row[3] == graph for row in rows)
first = source.read_quads(pattern, ('any', None), factories)
source.close()
rows = collect(first)
assert len(rows) == 450
assert all(isinstance(s, Literal) and isinstance(p, BNode) for s,p,o,g in rows)
assert {(type(g), str(g)) for s,p,o,g in rows} == {(type(g),str(g)) for g in graphs}
",
            Some(&module.dict()),
            None,
        )
        .unwrap();
    });
}

#[test]
fn full_quad_cursor_pins_one_version_and_releases_after_factory_failure() {
    Python::initialize();
    Python::attach(|py| {
        let module = super::import_tests::fixture(py);
        module.add_class::<NativeTermFactories>().unwrap();
        py.run(
            c"
def restore(value, datatype, language):
    return Literal(value, datatype=None if datatype is None else URIRef(datatype), lang=language)
factories = NativeTermFactories(URIRef, BNode, restore, str.__new__)
source = NativeStore.from_records(((URIRef('urn:s'+str(i)), URIRef('urn:p'), Literal('value'), None)
                                  for i in range(400)), (), inputs)
rows, cursor = source.read_quads((None,None,None), ('any', None), factories)
assert cursor is not None
source.insert_terms(('uri','urn:late',None,None), ('uri','urn:p',None,None),
                    ('literal','value',None,None), None)
while True:
    batch, done = cursor.next_batch()
    rows.extend(batch)
    if done: break
assert len(rows) == 400 and len(source) == 401
failure = ValueError('factory failure')
calls = 0
def broken(value, datatype, language):
    global calls
    calls += 1
    if calls > 128:
        cursor.close()
        raise failure
    return restore(value, datatype, language)
bad = NativeTermFactories(URIRef, BNode, broken, str.__new__)
rows, cursor = source.read_quads((None,None,None), ('any',None), bad)
try: cursor.next_batch()
except ValueError as error: assert error is failure
else: raise AssertionError('factory failure was hidden')
assert cursor.next_batch() == ([], True)
cursor.close()
",
            Some(&module.dict()),
            None,
        )
        .unwrap();
    });
}

#[test]
fn full_quad_scopes_reject_ambiguous_and_invalid_metadata() {
    Python::initialize();
    Python::attach(|py| {
        let module = super::import_tests::fixture(py);
        module.add_class::<NativeTermFactories>().unwrap();
        py.run(
            c"
factories = NativeTermFactories(URIRef, BNode, lambda *args: None, str.__new__)
source = NativeStore.from_records([], (), inputs)
for scope in [('any',('uri','urn:g',None,None)), ('graph',None), ('unknown',None),
              ('graph',('literal','g',None,None))]:
    try: source.read_quads((None,None,None), scope, factories)
    except ValueError: pass
    else: raise AssertionError('invalid graph scope accepted')
try:
    source.read_quads((('literal','bad','urn:datatype','en'),None,None), ('any',None), factories)
except ValueError: pass
else: raise AssertionError('malformed bound term accepted')
",
            Some(&module.dict()),
            None,
        )
        .unwrap();
    });
}

#[test]
fn union_and_exact_context_reads_are_bounded_owning_and_decode_only_selected_fields() {
    Python::initialize();
    Python::attach(|py| {
        let module = super::import_tests::fixture(py);
        module.add_class::<NativeTermFactories>().unwrap();
        py.run(
            c"
def restore(value, datatype, language):
    return Literal(value, datatype=None if datatype is None else URIRef(datatype), lang=language)
factories = NativeTermFactories(URIRef, BNode, restore, str.__new__)
def encode(value):
    if isinstance(value, Literal): return ('literal', str(value), value.datatype, value.language)
    return ('uri' if isinstance(value, URIRef) else 'bnode', str(value), None, None)
def fatal(*args): raise AssertionError('unselected statement terms were decoded')
context_factories = NativeTermFactories(URIRef, BNode, fatal, str.__new__)
def collect(first):
    rows, cursor = first
    assert len(rows) <= 128
    if cursor is not None:
        try:
            while True:
                batch, done = cursor.next_batch()
                assert len(batch) <= 128
                rows.extend(batch)
                if done: break
        finally: cursor.close()
    return rows
s,p = Literal('subject',lang='en'), BNode('predicate')
graphs = [None, URIRef('urn:scope'), BNode('urn:scope')]
records = [(s,p,Literal(str(i)),g) for i in range(400) for g in graphs]
extra = [kind('urn:context:'+str(i)) for i in range(150) for kind in [URIRef,BNode]]
records.extend((s,p,Literal('0'),g) for g in extra)
source = NativeStore.from_records(records, (), inputs)
union = source.read_union((None,None,None), factories)
contexts = source.read_contexts((encode(s),encode(p),encode(Literal('0'))), context_factories)
source.close()
pairs = collect(union)
rows = [statement for statement,contexts in pairs]
for statement,row_context in pairs: row_context.close()
assert len(rows) == 400 and len({str(o) for s,p,o in rows}) == 400
assert all(isinstance(s,Literal) and isinstance(p,BNode) for s,p,o in rows)
actual_contexts = collect(contexts)
assert len(actual_contexts) == 303
assert {(type(g),str(g)) for g in actual_contexts} == {(type(g),str(g)) for g in graphs+extra}
",
            Some(&module.dict()),
            None,
        )
        .unwrap();
    });
}

#[test]
fn exact_context_cursor_releases_after_reentrant_factory_failure_and_refuses_unbound_reads() {
    Python::initialize();
    Python::attach(|py| {
        let module = super::import_tests::fixture(py);
        module.add_class::<NativeTermFactories>().unwrap();
        py.run(c"
s,p,o = URIRef('urn:s'),URIRef('urn:p'),Literal('value')
source = NativeStore.from_records(((s,p,o,URIRef('urn:scope:'+str(i))) for i in range(300)), (), inputs)
pattern = (('uri','urn:s',None,None),('uri','urn:p',None,None),('literal','value',None,None))
failure = ValueError('synthetic context factory failure')
calls = 0
def allocate(cls,value):
    global calls
    calls += 1
    if calls > 128:
        cursor.close()
        raise failure
    return str.__new__(cls,value)
def fatal(*args): raise AssertionError('statement literal was restored')
factories = NativeTermFactories(URIRef,BNode,fatal,allocate)
rows,cursor = source.read_contexts(pattern,factories)
assert len(rows)==128 and cursor is not None
source.close()
try: cursor.next_batch()
except ValueError as error: assert error is failure
else: raise AssertionError('context restoration failure was hidden')
assert cursor.next_batch()==([],True)
cursor.close()
source = NativeStore.from_records([],(),inputs)
try: source.read_contexts((None,None,None),factories)
except ValueError: pass
else: raise AssertionError('unbound context scan was accepted')
", Some(&module.dict()), None).unwrap();
    });
}

#[test]
fn union_rows_own_lazy_contexts_from_the_same_snapshot_after_outer_and_store_close() {
    Python::initialize();
    Python::attach(|py| {
        let module = super::import_tests::fixture(py);
        module.add_class::<NativeTermFactories>().unwrap();
        py.run(c"
import json
restored_contexts = []
def allocate(cls,value):
    if value.startswith('urn:context:'): restored_contexts.append(value)
    return str.__new__(cls,value)
def restore(value,datatype,language):
    return Literal(value,datatype=None if datatype is None else URIRef(datatype),lang=language)
factories = NativeTermFactories(URIRef,BNode,restore,allocate)
graphs = [None,URIRef('urn:context:a'),BNode('urn:context:a')]
records = [(URIRef('urn:s'),URIRef('urn:p'),Literal(str(i)),g) for i in range(300) for g in graphs]
source = NativeStore.from_records(records,(),inputs)
for scope in [('default',None),('graph',('uri','urn:context:a',None,None)),('graph',('bnode','urn:context:a',None,None))]:
    selected,continuation=source.read_union((None,None,None),factories,scope=scope)
    while continuation is not None:
        extra,done=continuation.next_batch()
        selected.extend(extra)
        if done: continuation.close(); break
    assert len(selected)==300
    for statement,contexts in selected:
        batch,done=contexts.next_batch()
        assert done and {(type(g),str(g)) for g in batch}=={(type(g),str(g)) for g in graphs}
        contexts.close()
restored_contexts.clear()
rows,cursor = source.read_union((None,None,None),factories)
statement,contexts = rows[0]
abandoned_rows,abandoned_cursor=source.read_union((None,None,None),factories)
abandoned_context=abandoned_rows[0][1]
assert abandoned_cursor is not None
abandoned_cursor.close()
assert not restored_contexts
source.remove_pattern(json.dumps([None,None,None,json.dumps({'type':'uri','value':'urn:context:a'})]))
source.insert_terms(('uri','urn:s',None,None),('uri','urn:p',None,None),('literal','0',None,None),('uri','urn:context:late',None,None))
source.close()
while cursor is not None:
    batch,done = cursor.next_batch()
    assert len(batch)<=128
    rows.extend(batch)
    if done: cursor.close(); break
assert len(rows)==300 and not restored_contexts
expected = {(type(g),str(g)) for g in graphs}
for statement,contexts in rows:
    actual=[]
    while True:
        batch,done=contexts.next_batch()
        actual.extend(batch)
        if done: break
    contexts.close()
    assert {(type(g),str(g)) for g in actual}==expected
batch,done=abandoned_context.next_batch()
assert done and {(type(g),str(g)) for g in batch}==expected
abandoned_context.close()
assert 'urn:context:late' not in restored_contexts
", Some(&module.dict()), None).unwrap();
    });
}

#[test]
fn pending_context_selection_returns_all_contexts_from_one_worker_revision() {
    Python::initialize();
    Python::attach(|py| {
        let module = super::import_tests::fixture(py);
        py.run(c"
source=NativeStore.from_records([],(),inputs)
tx=source.transaction()
term=lambda kind,text:(kind,text,None,None)
s,p,o=term('literal','subject'),term('bnode','predicate'),term('literal','value')
graphs=[None,term('uri','urn:g'),term('bnode','urn:g')]
for graph in graphs: tx.insert_terms(s,p,o,graph)
for graph in ['',json.dumps({'type':'uri','value':'urn:g'}),json.dumps({'type':'bnode','value':'urn:g'}),None]:
    rows=json.loads(tx.match_context_quads(json.dumps([None,None,None,graph])))
    assert len(rows)==3, rows
assert json.loads(tx.match_context_quads(json.dumps([None,None,None,json.dumps({'type':'uri','value':'urn:absent'})])))==[]
assert json.loads(source.match_quads('[null,null,null,null]'))==[]
tx.rollback()
assert json.loads(source.match_quads('[null,null,null,null]'))==[]
",Some(&module.dict()),None).unwrap();
    });
}
