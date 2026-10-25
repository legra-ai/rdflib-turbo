//! Exercise actual source-owned binding classes, not an installed native wheel.
use std::ffi::CString;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};

fn execute(source: &str) {
    Python::initialize();
    Python::attach(|py| {
        let module = PyModule::new(py, "source_numeric_comparison").unwrap();
        crate::_native(&module).unwrap();
        let globals = PyDict::new(py);
        globals.set_item("native", module).unwrap();
        let source = CString::new(source).unwrap();
        py.run(&source, Some(&globals), None).unwrap();
    });
}
#[test]
fn caller_context_is_captured_for_every_validation_and_shared_owner_path() {
    execute(
        r#"
import decimal
import warnings
from rdflib import Graph
source = '<urn:s> <urn:p> "1.0"^^<http://www.w3.org/2001/XMLSchema#double>.'
shapes = '@prefix sh:<http://www.w3.org/ns/shacl#>. @prefix xsd:<http://www.w3.org/2001/XMLSchema#>. <urn:S> sh:targetNode <urn:s>; sh:property [sh:path <urn:p>;sh:minInclusive "0.9"^^xsd:decimal].'
compiled = native.shacl.CompiledShapes(shapes, 'turtle', None, False, True, [], _policy=native.shacl._REGEX_POLICY)
shared = native.SharedShapes.from_compiled(compiled)
store = native.NativeStore()
store.load(source, 'turtle')
legacy = native.NativeDataset()
legacy.load(source, 'turtle', None, True)
for trap in [False, True, False]:
    with decimal.localcontext() as context:
        context.traps[decimal.FloatOperation] = trap
        context.clear_flags()
        context.flags[decimal.Inexact] = True
        result = compiled.validate(source, 'turtle', None, False, 100, 15, False, False, False, False, [], [], [], _comparison_policy=native.shacl._VALUE_COMPARISON_POLICY)
        assert result[0] is (not trap)
        assert context.flags[decimal.FloatOperation]
        assert context.flags[decimal.Inexact]
        for operation in [lambda: shared.validate_store(store, '{}', False), lambda: shared.validate_store(store, '{}', True), lambda: shared.validate(legacy, '{}', False), lambda: shared.validate(legacy, '{}', True)]:
            context.clear_flags()
            context.flags[decimal.Inexact] = True
            assert operation()[0] is (not trap)
            assert context.flags[decimal.FloatOperation]
            assert context.flags[decimal.Inexact]
try:
    compiled.validate(source, 'turtle', None, False, 100, 15, False, False, False, False, [], [], [])
except TypeError as error:
    assert '_comparison_policy' in str(error)
else:
    raise AssertionError('Missing host authority accepted')
store.close()
"#,
    );
}
#[test]
fn actual_pair_warning_and_typed_errors_finish_before_rule_publication() {
    execute(
        r#"
import decimal
import warnings
import sys
source = '@prefix xsd:<http://www.w3.org/2001/XMLSchema#>. <urn:s> <urn:p> "1.0"^^xsd:double; <urn:q> "0.9"^^xsd:decimal.'
shapes = '@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:S> sh:targetNode <urn:s>;sh:rule [a sh:TripleRule;sh:subject sh:this;sh:predicate <urn:derived>;sh:object <urn:marker>;sh:condition <urn:C>]. <urn:C> sh:property [sh:path <urn:p>;sh:lessThanOrEquals <urn:q>].'
compiled = native.shacl.CompiledShapes(shapes, 'turtle', None, True, True, [], _policy=native.shacl._REGEX_POLICY)
shared = native.SharedShapes.from_compiled(compiled)
for legacy in [False, True]:
    owner = native.NativeDataset() if legacy else native.NativeStore()
    owner.load(source, 'turtle', None, True) if legacy else owner.load(source, 'turtle')
    before = owner.dump('nquads')
    with decimal.localcontext() as context, warnings.catch_warnings(record=True) as captured:
        context.traps[decimal.FloatOperation] = True
        context.clear_flags()
        warnings.filterwarnings('error', message='NotImplemented should not be used in a boolean context', category=DeprecationWarning)
        try:
            shared.validate(owner, '{}', True) if legacy else shared.validate_store(owner, '{}', True)
        except (DeprecationWarning, TypeError) as error:
            assert type(error) is (TypeError if sys.version_info >= (3,14) else DeprecationWarning)
            assert str(error) == 'NotImplemented should not be used in a boolean context'
        else:
            raise AssertionError('Numeric warning/error was accepted')
        assert context.flags[decimal.FloatOperation]
        assert not captured
    assert owner.dump('nquads') == before
    if not legacy: owner.close()
"#,
    );
}
