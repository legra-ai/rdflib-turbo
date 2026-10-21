"""Consumer acceptance owns the boundary between private rule output and writes."""

import pytest
from rdflib_turbo import Store
from rdflib_turbo._native import SharedShapes

SHAPES = """@prefix sh:<http://www.w3.org/ns/shacl#>.
<urn:S> sh:targetNode <urn:s>; sh:rule [a sh:TripleRule;
 sh:subject sh:this; sh:predicate <urn:derived>; sh:object true].
"""
SOURCE = "<urn:s> <urn:p> 1."


def test_report_callback_reads_only_original_pending_writes():
    program = SharedShapes(SHAPES, True, [])
    with Store() as store:
        with store.transaction() as tx:
            tx.parse(data=SOURCE)
            with tx._validate_shared_shapes(program, "{}", True) as result:
                assert result[0]
                assert len(tx) == 1
                assert not tx.query("ASK {<urn:s> <urn:derived> true}")
            assert len(tx) == 2
        assert len(store) == 2


@pytest.mark.parametrize("update", ["INSERT DATA {<urn:callback> <urn:p> 2}", "INSERT DATA {}"])
def test_report_callback_mutation_dooms_transaction_even_when_noop(update):
    program = SharedShapes(SHAPES, True, [])
    with Store() as store:
        with pytest.raises(RuntimeError, match="closed"), store.transaction() as tx:
            tx.parse(data=SOURCE)
            with pytest.raises(RuntimeError, match=r"changed.*validation"):
                with tx._validate_shared_shapes(program, "{}", True):
                    tx.update(update)
        assert len(store) == 0


def test_report_decode_error_discards_pending_original_and_rule_writes():
    program = SharedShapes(SHAPES, True, [])
    with Store() as store:
        with pytest.raises(RuntimeError, match="closed"), store.transaction() as tx:
            tx.parse(data=SOURCE)
            with pytest.raises(ValueError, match="decode"):
                with tx._validate_shared_shapes(program, "{}", True):
                    raise ValueError("decode")
        assert len(store) == 0
