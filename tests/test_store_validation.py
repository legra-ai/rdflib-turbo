"""SHACL uses the same native transaction as pending writes and inference."""

import pytest
from rdflib_turbo import Store
from rdflib_turbo._native import SharedShapes, SharedValidationFailure

SHAPES = """@prefix sh: <http://www.w3.org/ns/shacl#> .
<urn:S> sh:targetNode <urn:s>;
  sh:rule [a sh:TripleRule; sh:subject sh:this; sh:predicate <urn:derived>; sh:object true];
  sh:property [sh:path <urn:p>; sh:minCount 1];
  sh:property [sh:path <urn:derived>; sh:hasValue true].
"""
SOURCE = "<urn:s> <urn:p> 1."


def test_validation_reads_pending_writes_and_stages_only_rule_additions():
    shapes = SharedShapes(SHAPES, True, [])
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
    shapes = SharedShapes(SHAPES, True, [])
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
    shapes = SharedShapes(SHAPES, True, [])
    with Store() as store:
        with pytest.raises(RuntimeError, match="closed"):
            with store.transaction() as transaction:
                transaction.parse(data=SOURCE)
                with pytest.raises(ValueError):
                    shapes.validate_transaction(transaction._native, settings, True)
        assert len(store) == 0
