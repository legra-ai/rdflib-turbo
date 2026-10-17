"""SHACL borrows the dataset runtime; data never crosses a Python conversion."""

import pytest
from rdflib import Graph
from rdflib.compare import isomorphic
from rdflib_turbo import Dataset
from rdflib_turbo._native import SharedShapes

SHAPES = """@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix ex: <urn:example:> .
ex:S sh:targetNode ex:s; sh:property [sh:path ex:p; sh:minCount 1].
"""


def test_namespace_loading_is_atomic_and_survives_copies():
    data = Dataset().parse(data="@prefix ex: <urn:example:> . ex:s ex:p 1.")
    assert dict(data.namespaces())["ex"] == "urn:example:"
    assert dict(data.copy().namespaces()) == dict(data.namespaces())
    before = data.serialize(), tuple(data.namespaces())
    with pytest.raises(ValueError):
        data.parse(data="@prefix broken: <urn:broken:> . broken:s broken:p")
    assert (data.serialize(), tuple(data.namespaces())) == before


def test_shared_validation_reports_without_serializing_or_copying_dataset(monkeypatch):
    data = Dataset().parse(data="@prefix ex: <urn:example:> . ex:s ex:q 1.")
    identity = data._native
    program = SharedShapes(SHAPES, False, [("ex", "urn:example:")])

    def forbidden(*args, **kwargs):
        raise AssertionError("validation must borrow the existing native dataset")

    with monkeypatch.context() as patch:
        for name in ("copy", "serialize", "to_rdflib"):
            patch.setattr(Dataset, name, forbidden)
        conforms, report, text, warnings = program.validate(data._native, "{}", False)
    assert data._native is identity
    assert not conforms and "Conforms: False" in text
    assert not warnings
    graph = Graph().parse(data=report, format="nt")
    assert len(graph) > 1
    assert isomorphic(
        graph, Graph().parse(data=program.validate(data._native, "{}", False)[1], format="nt")
    )


def test_invalid_options_fail_and_subsequent_mutations_are_visible():
    data = Dataset().parse(data="<urn:example:s> <urn:example:q> 1.")
    program = SharedShapes(SHAPES, False, [])
    with pytest.raises(ValueError):
        program.validate(data._native, '{"unknown":true}', False)
    assert not program.validate(data._native, "{}", False)[0]
    data.parse(data="<urn:example:s> <urn:example:p> 2.")
    assert program.validate(data._native, "{}", False)[0]


def test_rules_stage_and_commit_only_when_requested():
    source = """@prefix sh: <http://www.w3.org/ns/shacl#> .
    <urn:S> sh:targetNode <urn:s>; sh:rule [a sh:TripleRule;
    sh:subject sh:this; sh:predicate <urn:p>; sh:object 2]."""
    data = Dataset().parse(data="<urn:s> <urn:q> 1.")
    program = SharedShapes(source, True, [])
    program.validate(data._native, "{}", False)
    assert len(data) == 1
    program.validate(data._native, "{}", True)
    assert len(data) == 2


def test_failed_rule_validation_never_commits():
    source = """@prefix sh: <http://www.w3.org/ns/shacl#> .
    <urn:S> sh:targetNode <urn:s>; sh:rule [a sh:TripleRule;
    sh:subject sh:this; sh:predicate <urn:p>; sh:object 2]."""
    data = Dataset().parse(data="<urn:s> <urn:q> 1.")
    before = data.serialize()
    with pytest.raises(ValueError, match="positive"):
        SharedShapes(source, True, []).validate(data._native, '{"max_rule_iterations":0}', True)
    assert data.serialize() == before
    with pytest.raises(RuntimeError, match="iteration limit"):
        SharedShapes(source, True, []).validate(
            data._native, '{"iterate_rules":true,"max_rule_iterations":1}', True
        )
    assert data.serialize() == before


def test_shared_program_is_reusable_across_concurrent_dataset_owners():
    from concurrent.futures import ThreadPoolExecutor

    program = SharedShapes(SHAPES, False, [])
    valid = Dataset().parse(data="<urn:example:s> <urn:example:p> 1.")
    invalid = Dataset().parse(data="<urn:example:s> <urn:example:q> 1.")
    with ThreadPoolExecutor(max_workers=4) as pool:
        results = list(
            pool.map(
                lambda i: program.validate((valid if i % 2 else invalid)._native, "{}", False)[0],
                range(20),
            )
        )
    assert results == [bool(i % 2) for i in range(20)]


def test_unknown_settings_and_invalid_focus_iris_are_hard_errors():
    data = Dataset()
    program = SharedShapes(SHAPES, False, [])
    for settings in ('{"focus_nodes":["not an IRI"]}', '{"max_rule_iterations":-1}', "[]"):
        with pytest.raises(ValueError):
            program.validate(data._native, settings, False)
