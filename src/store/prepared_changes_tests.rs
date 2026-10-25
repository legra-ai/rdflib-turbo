//! Python-visible inference output must succeed before caller publication.
use super::super::term_output::NativeTermFactories;
use pyo3::prelude::*;

#[test]
fn prepared_inference_changes_abort_until_explicit_complete_acceptance() {
    Python::initialize();
    Python::attach(|py| {
        let module = super::super::import_tests::fixture(py);
        module.add_class::<NativeTermFactories>().unwrap();
        py.run(
            c"
def restore(value, datatype, language):
    return Literal(value, datatype=None if datatype is None else URIRef(datatype), lang=language)
factories = NativeTermFactories(URIRef, BNode, restore, str.__new__)
statement = (URIRef('urn:s'), BNode('predicate'), URIRef('urn:o'), None)
source = NativeStore.from_records([statement], (), inputs)
prepared = source._prepare_inference_changes('owlrl', (None,''), factories)
assert len(source) == 1
try: prepared.accept()
except RuntimeError: pass
else: raise AssertionError('unconsumed changes were accepted')
prepared.close()
assert len(source) == 1
prepared = source._prepare_inference_changes('owlrl', (None,''), factories)
added = []
for method in ['next_additions', 'next_removals']:
    while True:
        rows, done = getattr(prepared, method)()
        if method == 'next_additions': added.extend(rows)
        if done: break
    assert getattr(prepared, method)() == ([],True)
assert len(source) == 1 and added
prepared.accept()
assert len(source) > 1
try: prepared.accept()
except RuntimeError: pass
else: raise AssertionError('changes were accepted twice')
prepared.close()
",
            Some(&module.dict()),
            None,
        )
        .unwrap();
    });
}

#[test]
fn prepared_changes_reject_reentrant_acceptance_and_abort_factory_failure() {
    Python::initialize();
    Python::attach(|py| {
        let module = super::super::import_tests::fixture(py);
        module.add_class::<NativeTermFactories>().unwrap();
        py.run(c"
source = NativeStore.from_records([(URIRef('urn:s'), URIRef('urn:p'), Literal('value'), None)], (), inputs)
failure = ValueError('synthetic prepared restoration failure')
attempts = 0
def broken(value, datatype, language):
    global attempts
    attempts += 1
    try: prepared.discard_removals()
    except RuntimeError: pass
    else: raise AssertionError('discard ran inside a term factory')
    try: prepared.accept()
    except RuntimeError: pass
    else: raise AssertionError('acceptance ran inside a term factory')
    prepared.close()
    raise failure
factories = NativeTermFactories(URIRef, BNode, broken, str.__new__)
prepared = source._prepare_inference_changes('rdfs', (None,''), factories)
while True:
    rows, done = prepared.next_removals()
    if done: break
try:
    while True:
        rows, done = prepared.next_additions()
        if done: break
except ValueError as error: assert error is failure
else: raise AssertionError('restoration failure was hidden')
assert attempts and len(source) == 1
prepared.close()
prepared = source._prepare_inference_changes('rdfs', (None,''), factories)
del prepared
import gc
gc.collect()
assert len(source) == 1
source.close()
", Some(&module.dict()), None).unwrap();
    });
}

#[test]
fn prepared_additions_only_discard_does_not_decode_removed_literal_terms() {
    Python::initialize();
    Python::attach(|py| {
        let module = super::super::import_tests::fixture(py);
        module.add_class::<NativeTermFactories>().unwrap();
        py.run(
            c"
statement = (Literal('removed literal subject'), BNode('predicate'), URIRef('urn:o'), None)
source = NativeStore.from_records([statement], (), inputs)
def fatal(*args): raise AssertionError('removed literal was restored')
bad = NativeTermFactories(URIRef, BNode, fatal, str.__new__)
prepared = source._prepare_inference_changes('owlrl', (None,''), bad)
prepared.discard_removals()
prepared.discard_removals()
assert prepared.next_removals() == ([],True)
assert len(source) == 1
try: prepared.accept()
except RuntimeError: pass
else: raise AssertionError('unconsumed additions were accepted')
prepared.close()
assert len(source) == 1
try: prepared.discard_removals()
except RuntimeError: pass
else: raise AssertionError('closed changes accepted discard')
def restore(value,datatype,language):
    return Literal(value, datatype=None if datatype is None else URIRef(datatype), lang=language)
good = NativeTermFactories(URIRef,BNode,restore,str.__new__)
prepared = source._prepare_inference_changes('owlrl', (None,''), good)
while True:
    rows, done = prepared.next_additions()
    if done: break
prepared.discard_removals()
prepared.accept()
assert len(source) > 1
try: prepared.discard_removals()
except RuntimeError: pass
else: raise AssertionError('accepted changes accepted discard')
prepared.close()
",
            Some(&module.dict()),
            None,
        )
        .unwrap();
    });
}
