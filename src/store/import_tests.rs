//! The import factory owns one unpublished writer until both streams succeed.
use pyo3::prelude::*;
use pyo3::types::PyModule;

use super::NativeStore;
use super::input_terms::NativeInputTerms;

pub(super) fn fixture(py: Python<'_>) -> Bound<'_, PyModule> {
    let module = PyModule::new(py, "native_import_fixture").unwrap();
    py.run(
        c"
import json
class URIRef(str): pass
class BNode(str): pass
class Literal(str):
    def __new__(cls, value, *, datatype=None, lang=None, normalize=True):
        term = super().__new__(cls, value)
        term.datatype, term.language = datatype, lang
        return term
",
        Some(&module.dict()),
        None,
    )
    .unwrap();
    module.add_class::<NativeStore>().unwrap();
    module.add_class::<NativeInputTerms>().unwrap();
    py.run(
        c"inputs = NativeInputTerms(URIRef, BNode, Literal)",
        Some(&module.dict()),
        None,
    )
    .unwrap();
    module
}

#[test]
fn streamed_records_preserve_full_terms_and_declared_graph_domains() {
    Python::initialize();
    Python::attach(|py| {
        let module = fixture(py);
        py.run(
            c"
roles = [URIRef('urn:role'), BNode('role'), Literal('role', lang='en')]
graphs = [None, URIRef('urn:scope'), BNode('urn:scope')]
empty = [URIRef('urn:empty'), BNode('urn:empty')]
value = Literal('01', datatype=URIRef('http://www.w3.org/2001/XMLSchema#integer'), normalize=False)
records = [(s,p,value,g) for s in roles for p in roles for g in graphs]
result = NativeStore.from_records(iter(records), iter(graphs + empty), inputs)
def encode(term):
    if term is None: return None
    if isinstance(term, URIRef): return {'type':'uri','value':str(term)}
    if isinstance(term, BNode): return {'type':'bnode','value':str(term)}
    output = {'type':'literal','value':str(term)}
    if term.datatype is not None: output['datatype'] = str(term.datatype)
    if term.language is not None: output['xml:lang'] = term.language
    return output
expected = {json.dumps([encode(t) for t in row], sort_keys=True) for row in records}
actual = {json.dumps(row, sort_keys=True) for row in json.loads(result.match_quads('[null,null,null,null]'))}
assert actual == expected
assert len(result) == 27
assert set(json.loads(result.named_graphs())) == {'<urn:scope>', '_:urn:scope', '<urn:empty>', '_:urn:empty'}
",
            Some(&module.dict()),
            None,
        )
        .unwrap();
    });
}

#[test]
fn import_failures_preserve_python_exception_identity_and_graph_domain_errors() {
    Python::initialize();
    Python::attach(|py| {
        let module = fixture(py);
        py.run(
            c"
failure = ValueError('synthetic import failure')
def broken():
    for n in range(129):
        yield URIRef('urn:s:' + str(n)), URIRef('urn:p'), Literal('value'), None
    raise failure
def broken_graphs():
    for n in range(129):
        yield URIRef('urn:graph:' + str(n))
    raise failure
for records, graphs in [(broken(), ()), ((), broken_graphs())]:
    try:
        NativeStore.from_records(records, graphs, inputs)
    except ValueError as error:
        assert error is failure
    else:
        raise AssertionError('iterator error did not propagate')
for records, graphs in [([(URIRef('urn:s'), URIRef('urn:p'), Literal('v'), Literal('bad'))], ()), ((), [Literal('bad')])]:
    try:
        NativeStore.from_records(records, graphs, inputs)
    except ValueError as error:
        assert 'Graph name' in str(error)
    else:
        raise AssertionError('literal graph was accepted')
",
            Some(&module.dict()),
            None,
        )
        .unwrap();
    });
}

#[test]
fn import_releases_each_bounded_input_window_and_propagates_metadata_errors() {
    Python::initialize();
    Python::attach(|py| {
        let module = fixture(py);
        py.run(
            c"
import weakref
references = []
peak = 0
class Tracked(URIRef): pass
def records():
    global peak
    # bounded: 513 test records cross four complete 128-record windows.
    for n in range(513):
        term = Tracked('urn:s:' + str(n))
        references.append(weakref.ref(term))
        peak = max(peak, sum(ref() is not None for ref in references))
        yield term, URIRef('urn:p'), Literal('value'), None
result = NativeStore.from_records(records(), (), inputs)
assert len(result) == 513
assert peak <= 129, peak
assert all(ref() is None for ref in references)
failure = ValueError('synthetic language metadata failure')
class BrokenLiteral(Literal):
    def __new__(cls): return str.__new__(cls, 'value')
    @property
    def datatype(self): return None
    @property
    def language(self): raise failure
def invalid_records():
    for n in range(129):
        yield URIRef('urn:s:' + str(n)), URIRef('urn:p'), Literal('value'), None
    yield URIRef('urn:bad'), URIRef('urn:p'), BrokenLiteral(), None
try:
    NativeStore.from_records(invalid_records(), (), inputs)
except ValueError as error:
    assert error is failure
else:
    raise AssertionError('metadata error did not propagate')
",
            Some(&module.dict()),
            None,
        )
        .unwrap();
    });
}
