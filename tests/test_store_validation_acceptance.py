"""Consumer acceptance owns the boundary between private rule output and writes."""

import pytest
from rdflib_turbo import Graph, Store
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


def test_scoped_empty_inference_attempt_invalidates_report_acceptance():
    program = SharedShapes(SHAPES, True, [])
    with Store() as store:
        with pytest.raises(RuntimeError, match="closed"), store.transaction() as tx:
            tx.parse(data=SOURCE)
            with pytest.raises(RuntimeError, match=r"changed.*validation"):
                with tx._validate_shared_shapes(program, "{}", True):
                    Graph(store=tx, identifier="urn:empty", bind_namespaces="none").infer(
                        inference="rdfs"
                    )
                    assert len(tx) == 1
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


WARNING_SHAPES = (
    SHAPES
    + """@prefix ex:<urn:>.
<urn:S> sh:rule [a sh:TripleRule; sh:condition ex:C;
 sh:subject sh:this; sh:predicate ex:derived; sh:object true].
ex:C sh:node ex:C.
"""
)


def test_successful_nonconformance_accepts_rules_outside_a_commit_gate():
    shapes = SHAPES + "<urn:S> sh:property [sh:path <urn:missing>; sh:minCount 1]."
    program = SharedShapes(shapes, True, [])
    with Store() as store:
        store.parse(data=SOURCE)
        with store._validate_shared_shapes(program, "{}", True) as result:
            assert not result[0]
        assert len(store) == 2


def test_real_warning_location_matches_ordinary_graph():
    import warnings

    from pyshacl_turbo import ShapeRecursionWarning, compile_shapes
    from rdflib import Graph

    program = compile_shapes(Graph().parse(data=WARNING_SHAPES, format="turtle"), advanced=True)

    def invoke(source):
        return program.validate(source, inplace=True)

    captures = []
    with Store() as store:
        store.parse(data=SOURCE)
        for source in (Graph().parse(data=SOURCE, format="turtle"), store):
            with warnings.catch_warnings(record=True) as emitted:
                warnings.simplefilter("always", ShapeRecursionWarning)
                invoke(source)
            captures.append([(w.category, str(w.message), w.filename, w.lineno) for w in emitted])
    assert captures[0]
    assert captures[0] == captures[1]


@pytest.mark.parametrize("scenario", ["read", "mutate", "concurrent", "error"])
def test_real_warning_callback_obeys_staged_acceptance_without_deadlock(scenario):
    import subprocess
    import sys

    subprocess.run(
        [
            sys.executable,
            "-c",
            r"""
import sys
import threading
import warnings
from pyshacl_turbo import ShapeRecursionWarning, compile_shapes
from rdflib import Graph
from rdflib_turbo import Store

shapes, scenario = sys.argv[1:]
program = compile_shapes(Graph().parse(data=shapes, format="turtle"), advanced=True)
callbacks = []
with Store() as store:
    try:
        with store.transaction() as tx:
            tx.parse(data="<urn:s> <urn:p> 1.")
            def hook(*args, **kwargs):
                callbacks.append(1)
                assert len(tx) == 1, "unaccepted derived facts were visible"
                assert tx.query("ASK {<urn:s> <urn:p> 1}")
                if scenario == "mutate":
                    tx.update("INSERT DATA {}")
                elif scenario == "concurrent":
                    errors = []
                    def mutate():
                        try:
                            tx.update("INSERT DATA {}")
                        except BaseException as error:
                            errors.append(error)
                    worker = threading.Thread(target=mutate, daemon=True)
                    worker.start()
                    worker.join(3)
                    assert not worker.is_alive(), "warning callback held the session lock"
                    assert not errors, errors
            with warnings.catch_warnings():
                warnings.simplefilter("error" if scenario == "error" else "always", ShapeRecursionWarning)
                warnings.showwarning = hook
                program.validate(tx, inplace=True)
            assert scenario == "read", "mutation was silently accepted"
            assert len(tx) == 2
    except ShapeRecursionWarning:
        assert scenario == "error"
    except RuntimeError as error:
        assert scenario in ("mutate", "concurrent")
        assert "changed while processing validation" in str(error), str(error)
    assert len(store) == (2 if scenario == "read" else 0)
assert callbacks or scenario == "error"
""",
            WARNING_SHAPES,
            scenario,
        ],
        check=True,
        timeout=12,
    )
