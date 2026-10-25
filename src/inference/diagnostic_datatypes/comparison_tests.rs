//! Exercise actual source-owned binding classes, not an installed native wheel.
use std::ffi::CString;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};

#[pyfunction]
fn legacy_revision(owner: PyRef<'_, crate::NativeDataset>) -> PyResult<u64> {
    owner
        .inner
        .read()
        .map(|state| state.revision)
        .map_err(|_| pyo3::exceptions::PyRuntimeError::new_err("Dataset lock poisoned"))
}

#[pyfunction]
fn force_factory_construction(py: Python<'_>) -> PyResult<()> {
    super::Host::new(py).map(|_| ())
}

fn execute(source: &str) {
    Python::initialize();
    Python::attach(|py| {
        let module = PyModule::new(py, "source_numeric_comparison").unwrap();
        crate::_native(&module).unwrap();
        let globals = PyDict::new(py);
        globals.set_item("native", module).unwrap();
        globals
            .set_item(
                "force_factory_construction",
                wrap_pyfunction!(force_factory_construction, py).unwrap(),
            )
            .unwrap();
        globals
            .set_item(
                "legacy_revision",
                wrap_pyfunction!(legacy_revision, py).unwrap(),
            )
            .unwrap();
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
shared = native.SharedShapes.from_compiled(compiled, _comparison_policy=native.shacl._VALUE_COMPARISON_POLICY)
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
shared = native.SharedShapes.from_compiled(compiled, _comparison_policy=native.shacl._VALUE_COMPARISON_POLICY)
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

#[test]
fn late_python_warning_failure_cannot_publish_staged_rules_or_mutate_owner_metadata() {
    execute(
        r#"
import decimal
import warnings
import sys
source = '@prefix kept:<urn:kept:>. @prefix xsd:<http://www.w3.org/2001/XMLSchema#>. <urn:s> <urn:p> "1.0"^^xsd:double; <urn:q> "0.9"^^xsd:decimal.'
shapes = '@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:S> sh:targetNode <urn:s>;sh:rule [a sh:TripleRule;sh:subject sh:this;sh:predicate <urn:derived>;sh:object <urn:marker>;sh:condition <urn:C>]. <urn:C> sh:property [sh:path <urn:p>;sh:lessThanOrEquals <urn:q>].'
compiled = native.shacl.CompiledShapes(shapes, 'turtle', None, True, True, [], _policy=native.shacl._REGEX_POLICY)
shared = native.SharedShapes.from_compiled(compiled, _comparison_policy=native.shacl._VALUE_COMPARISON_POLICY)
for legacy in [False, True]:
    owner = native.NativeDataset() if legacy else native.NativeStore()
    owner.load(source, 'turtle', None, True) if legacy else owner.load(source, 'turtle')
    before = owner.dump('nquads')
    namespaces = owner.namespaces() if legacy else None
    revision = legacy_revision(owner) if legacy else None
    sentinel = RuntimeError('late Core numeric warning publisher sentinel')
    observed = []
    def fail_after_native_work(*args, **kwargs):
        observed.append(args)
        # This read also proves no native owner mutex is held across the callback.
        assert owner.dump('nquads') == before
        raise sentinel
    with decimal.localcontext() as context, warnings.catch_warnings():
        context.clear_flags()
        context.traps[decimal.FloatOperation] = True
        warnings.filterwarnings('always', message='NotImplemented should not be used in a boolean context', category=DeprecationWarning)
        warnings.showwarning = fail_after_native_work
        try:
            shared.validate(owner, '{}', True) if legacy else shared.validate_store(owner, '{}', True)
        except RuntimeError as error:
            assert sys.version_info < (3,14)
            assert error is sentinel
            assert len(observed) == 1
        except TypeError as error:
            assert sys.version_info >= (3,14)
            assert str(error) == 'NotImplemented should not be used in a boolean context'
            assert not observed
        else:
            raise AssertionError('Late diagnostic failure was accepted')
        assert context.flags[decimal.FloatOperation]
    assert owner.dump('nquads') == before
    if legacy:
        assert owner.namespaces() == namespaces
        assert legacy_revision(owner) == revision
    else:
        # The failed pending writer and its private changeset must be gone.
        transaction = owner.transaction()
        transaction.rollback()
    if sys.version_info < (3,14):
        with decimal.localcontext() as context, warnings.catch_warnings(record=True) as emitted:
            context.traps[decimal.FloatOperation] = True
            warnings.filterwarnings('always', message='NotImplemented should not be used in a boolean context', category=DeprecationWarning)
            result = shared.validate(owner, '{}', True) if legacy else shared.validate_store(owner, '{}', True)
            assert result[0]
            assert len(emitted) == 1
        # The identical successful worker really derives the rule marker.
        assert '<urn:s> <urn:derived> <urn:marker>' in owner.dump('nquads')
    if not legacy: owner.close()
"#,
    );
}

#[test]
fn native_warning_uses_the_real_host_site_without_repeated_factory_introspection() {
    execute(
        r#"
import decimal
import dis
import sys
import warnings
from rdflib import Literal, XSD
left = Literal(1.0, datatype=XSD.double)
right = Literal(decimal.Decimal('0.9'), datatype=XSD.decimal)
with decimal.localcontext() as context, warnings.catch_warnings(record=True) as original:
    context.traps[decimal.FloatOperation] = True
    warnings.filterwarnings('always', message='NotImplemented should not be used in a boolean context', category=DeprecationWarning)
    try:
        assert left <= right
    except TypeError:
        assert sys.version_info >= (3,14)
        assert not original
    else:
        assert sys.version_info < (3,14)
        assert len(original) == 1
source = '@prefix xsd:<http://www.w3.org/2001/XMLSchema#>. <urn:s> <urn:p> "1.0"^^xsd:double; <urn:q> "0.9"^^xsd:decimal.'
shapes = '@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:S> sh:targetNode <urn:s>;sh:property [sh:path <urn:p>;sh:lessThanOrEquals <urn:q>].'
compiled = native.shacl.CompiledShapes(shapes, 'turtle', None, False, True, [], _policy=native.shacl._REGEX_POLICY)
shared = native.SharedShapes.from_compiled(compiled, _comparison_policy=native.shacl._VALUE_COMPARISON_POLICY)
store = native.NativeStore()
store.load(source, 'turtle')
legacy = native.NativeDataset()
legacy.load(source, 'turtle', None, True)
old_dis = dis.get_instructions
sentinel = RuntimeError('factory introspection must not run per validation')
def forbidden(*args, **kwargs):
    raise sentinel
dis.get_instructions = forbidden
try:
    # A negative control proves the spy really detects factory reconstruction.
    try:
        force_factory_construction()
    except RuntimeError as error:
        assert error is sentinel
    else:
        raise AssertionError('Factory introspection guard is ineffective')
    for operation in [lambda: compiled.validate(source, 'turtle', None, False, 100, 15, False, False, False, False, [], [], [], _comparison_policy=native.shacl._VALUE_COMPARISON_POLICY), lambda: shared.validate_store(store, '{}', False), lambda: shared.validate(legacy, '{}', False)]:
        with decimal.localcontext() as context, warnings.catch_warnings(record=True) as emitted:
            context.traps[decimal.FloatOperation] = True
            context.clear_flags()
            warnings.filterwarnings('always', message='NotImplemented should not be used in a boolean context', category=DeprecationWarning)
            try:
                result = operation()
            except TypeError as error:
                assert sys.version_info >= (3,14)
                assert str(error) == 'NotImplemented should not be used in a boolean context'
                assert not emitted
            else:
                assert sys.version_info < (3,14)
                assert result[0]
                assert len(emitted) == 1
                assert emitted[0].category is original[0].category
                assert str(emitted[0].message) == str(original[0].message)
                assert emitted[0].filename == original[0].filename
                assert emitted[0].lineno == original[0].lineno
            assert context.flags[decimal.FloatOperation]
finally:
    dis.get_instructions = old_dis
    store.close()
"#,
    );
}
