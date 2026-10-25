"""SHACL uses the same native transaction as pending writes and inference."""

import json
import subprocess
import sys

import pytest
from rdflib_turbo import Dataset, Store
from rdflib_turbo._native import SharedShapes, SharedValidationFailure
from rdflib_turbo._native import shacl as _native_shacl


def test_committed_validation_does_not_wait_for_an_open_writer():
    subprocess.run(
        [
            sys.executable,
            "-c",
            """
from rdflib_turbo import Store
from rdflib_turbo._native import SharedShapes
from rdflib_turbo._native import shacl as _native_shacl
program = SharedShapes("", False, [], _policy=_native_shacl._REGEX_POLICY)
with Store() as store:
    with store.transaction() as tx:
        tx.parse(data="<urn:s> <urn:p> 1.")
        with store._validate_shared_shapes(program, "{}", False) as result:
            assert result[0]
""",
        ],
        check=True,
        timeout=5,
    )


SHAPES = """@prefix sh: <http://www.w3.org/ns/shacl#> .
<urn:S> sh:targetNode <urn:s>;
  sh:rule [a sh:TripleRule; sh:subject sh:this; sh:predicate <urn:derived>; sh:object true];
  sh:property [sh:path <urn:p>; sh:minCount 1];
  sh:property [sh:path <urn:derived>; sh:hasValue true].
"""
SOURCE = "<urn:s> <urn:p> 1."


def test_validation_reads_pending_writes_and_stages_only_rule_additions():
    shapes = SharedShapes(SHAPES, True, [], _policy=_native_shacl._REGEX_POLICY)
    with Store() as store:
        with store.transaction() as transaction:
            transaction.parse(data=SOURCE)
            assert shapes.validate_transaction(transaction._native, "{}", False)[0]
            assert not transaction.query("ASK {<urn:s> <urn:derived> true}")
            assert shapes.validate_transaction(transaction._native, "{}", True)[0]
            assert transaction.query("ASK {<urn:s> <urn:derived> true}")
            assert len(store) == 0
        assert len(store) == 2


@pytest.mark.parametrize("inplace", [False, True])
def test_store_validation_only_commits_when_requested(inplace):
    shapes = SharedShapes(SHAPES, True, [], _policy=_native_shacl._REGEX_POLICY)
    with Store() as store:
        store.parse(data=SOURCE)
        assert shapes.validate_store(store._native, "{}", inplace)[0]
        assert len(store) == (2 if inplace else 1)


def test_native_validation_failure_dooms_the_whole_transaction_even_when_caught():
    shapes = SharedShapes(
        SHAPES.replace(
            "sh:property [sh:path <urn:p>; sh:minCount 1];",
            'sh:sparql [sh:select "SELECT $this (true AS ?failure) WHERE {} "];',
        ),
        True,
        [],
        _policy=_native_shacl._REGEX_POLICY,
    )
    with Store() as store:
        with pytest.raises(RuntimeError, match="closed"):
            with store.transaction() as transaction:
                transaction.parse(data=SOURCE)
                with pytest.raises(SharedValidationFailure):
                    shapes.validate_transaction(transaction._native, "{}", True)
        assert len(store) == 0


@pytest.mark.parametrize("settings", ['{"unknown":true}', "[]", '{"max_validation_depth":-1}'])
def test_invalid_native_validation_settings_abort_pending_writes(settings):
    shapes = SharedShapes(SHAPES, True, [], _policy=_native_shacl._REGEX_POLICY)
    with Store() as store:
        with pytest.raises(RuntimeError, match="closed"):
            with store.transaction() as transaction:
                transaction.parse(data=SOURCE)
                with pytest.raises(ValueError):
                    shapes.validate_transaction(transaction._native, settings, True)
        assert len(store) == 0


class Program:
    """Test the commit gate without depending on the optional facade package."""

    def __init__(self):
        self.native = SharedShapes(SHAPES, True, [], _policy=_native_shacl._REGEX_POLICY)

    def validate(self, source, *, inplace, **options):
        with source._validate_shared_shapes(self.native, json.dumps(options), inplace) as result:
            conforms, report, text, _ = result
            return conforms, report, text


@pytest.mark.parametrize("explicit", [False, True])
def test_report_consumer_failure_aborts_owner(explicit):
    program = SharedShapes(SHAPES, True, [], _policy=_native_shacl._REGEX_POLICY)
    with Store() as store:
        if explicit:
            with pytest.raises(RuntimeError, match="closed"), store.transaction() as transaction:
                transaction.parse(data=SOURCE)
                with pytest.raises(ValueError, match="report consumer"):
                    with transaction._validate_shared_shapes(program, "{}", True):
                        raise ValueError("report consumer")
        else:
            store.parse(data=SOURCE)
            with pytest.raises(ValueError, match="report consumer"):
                with store._validate_shared_shapes(program, "{}", True):
                    raise ValueError("report consumer")
            assert len(store) == 1
            return
        assert len(store) == 0


def test_validated_transaction_commits_original_and_rule_writes_together():
    with Store() as store:
        with store.transaction(shapes=Program(), iterate_rules=True) as transaction:
            transaction.parse(data=SOURCE)
        assert len(store) == 2


def test_direct_inplace_store_validation_commits_after_report_consumption():
    with Store() as store:
        store.parse(data=SOURCE)
        conforms, report, text = Program().validate(store, inplace=True)
        assert conforms and report and "Conforms: True" in text
        assert store.query("ASK {<urn:s> <urn:derived> true}")
        assert len(store) == 2


def test_nonconforming_commit_gate_aborts_every_pending_write_and_retains_report():
    from rdflib_turbo import ValidationError

    with Store() as store:
        store.parse(data="<urn:keep> <urn:p> 2.")
        with pytest.raises(ValidationError) as failure:
            with store.transaction(shapes=Program()) as transaction:
                transaction.parse(data="<urn:s> <urn:other> 1.")
        assert failure.value.report
        assert "Conforms: False" in failure.value.text
        assert len(store) == 1
        assert store.query("ASK {<urn:keep> <urn:p> 2}")


def test_commit_gate_rejects_options_without_shapes_before_starting_a_writer():
    with Store() as store:
        with pytest.raises(TypeError, match="shapes"):
            with store.transaction(iterate_rules=True):
                pytest.fail("invalid gate must fail before user writes")
        with pytest.raises(TypeError, match="inplace"):
            with store.transaction(shapes=Program(), inplace=False):
                pytest.fail("gate must require transaction-local writes")


@pytest.mark.parametrize("kind", ["dataset", "store", "transaction"])
def test_sources_dispatch_validation_to_their_owned_native_backend(kind):
    program = SharedShapes(SHAPES, True, [], _policy=_native_shacl._REGEX_POLICY)
    if kind == "dataset":
        source = Dataset().parse(data=SOURCE)
        with source._validate_shared_shapes(program, "{}", False) as result:
            assert result[0]
        assert len(source) == 1
        return
    with Store() as store:
        if kind == "store":
            store.parse(data=SOURCE)
            with store._validate_shared_shapes(program, "{}", False) as result:
                assert result[0]
        else:
            with store.transaction() as transaction:
                transaction.parse(data=SOURCE)
                with transaction._validate_shared_shapes(program, "{}", False) as result:
                    assert result[0]
        assert len(store) == 1


def test_invalid_compiled_shapes_fail_before_starting_the_transaction():
    with Store() as store:
        with pytest.raises(TypeError, match="compiled validation"):
            with store.transaction(shapes=object()):
                pytest.fail("invalid shapes must fail before user writes")
