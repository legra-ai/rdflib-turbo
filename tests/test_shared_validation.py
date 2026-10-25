"""SHACL borrows the dataset runtime; data never crosses a Python conversion."""

import tomllib
from pathlib import Path

import pytest
from packaging.requirements import Requirement
from rdflib import BNode, Graph, Literal, URIRef
from rdflib.compare import isomorphic
from rdflib_turbo import Dataset
from rdflib_turbo._native import NativeDataset, SharedShapes
from rdflib_turbo._native import shacl as _native_shacl


def load_dataset(data, source):
    data.load(source, "turtle", None, True)
    return data


def native_dataset(source):
    return load_dataset(NativeDataset(), source)


SHAPES = """@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix ex: <urn:example:> .
ex:S sh:targetNode ex:s; sh:property [sh:path ex:p; sh:minCount 1].
"""


def report_graph(batch):
    records, quads = batch
    terms = []
    for kind, value, datatype, language in records:
        assert kind in (0, 1, 2)
        terms.append(
            Literal(value, lang=language, datatype=datatype, normalize=False)
            if kind == 2
            else (URIRef(value) if kind == 0 else BNode(value))
        )
    graph = Graph()
    for subject, predicate, obj, context in quads:
        assert context is None
        graph.add((terms[subject], terms[predicate], terms[obj]))
    return graph


def test_namespace_loading_is_atomic_and_survives_copies():
    data = native_dataset("@prefix ex: <urn:example:> . ex:s ex:p 1.")
    assert dict(data.namespaces())["ex"] == "urn:example:"
    assert dict(data.copy().namespaces()) == dict(data.namespaces())
    before = data.dump("nquads"), tuple(data.namespaces())
    with pytest.raises(ValueError):
        load_dataset(data, "@prefix broken: <urn:broken:> . broken:s broken:p")
    assert (data.dump("nquads"), tuple(data.namespaces())) == before
    load_dataset(data, "@prefix ex: <urn:updated:> .")
    assert dict(data.namespaces())["ex"] == "urn:updated:"
    assert len(data) == 1
    snapshot = tuple(data.namespaces())
    with pytest.raises(ValueError):
        data.bind_namespaces([("invalid", "not an IRI")])
    assert tuple(data.namespaces()) == snapshot


def test_shared_validation_reports_without_serializing_or_copying_dataset(monkeypatch):
    data = native_dataset("@prefix ex: <urn:example:> . ex:s ex:q 1.")
    identity = data
    program = SharedShapes(
        SHAPES, False, [("ex", "urn:example:")], _policy=_native_shacl._REGEX_POLICY
    )

    def forbidden(*args, **kwargs):
        raise AssertionError("validation must borrow the existing native dataset")

    with monkeypatch.context() as patch:
        for name in ("copy", "serialize", "to_rdflib"):
            patch.setattr(Dataset, name, forbidden)
        conforms, report, text, warnings = program.validate(data, "{}", False)
    assert data is identity
    assert not conforms and "Conforms: False" in text
    assert not warnings
    assert "Focus Node: ex:s" in text
    graph = report_graph(report)
    assert len(graph) > 1
    assert isomorphic(graph, report_graph(program.validate(data, "{}", False)[1]))


def test_invalid_options_fail_and_subsequent_mutations_are_visible():
    data = native_dataset("<urn:example:s> <urn:example:q> 1.")
    program = SharedShapes(SHAPES, False, [], _policy=_native_shacl._REGEX_POLICY)
    with pytest.raises(ValueError):
        program.validate(data, '{"unknown":true}', False)
    assert not program.validate(data, "{}", False)[0]
    load_dataset(data, "<urn:example:s> <urn:example:p> 2.")
    assert program.validate(data, "{}", False)[0]


def test_rules_stage_and_commit_only_when_requested():
    source = """@prefix sh: <http://www.w3.org/ns/shacl#> .
    <urn:S> sh:targetNode <urn:s>; sh:rule [a sh:TripleRule;
    sh:subject sh:this; sh:predicate <urn:p>; sh:object 2]."""
    data = native_dataset("<urn:s> <urn:q> 1.")
    program = SharedShapes(source, True, [], _policy=_native_shacl._REGEX_POLICY)
    program.validate(data, "{}", False)
    assert len(data) == 1
    program.validate(data, "{}", True)
    assert len(data) == 2


def test_failed_rule_validation_never_commits():
    source = """@prefix sh: <http://www.w3.org/ns/shacl#> .
    <urn:S> sh:targetNode <urn:s>; sh:rule [a sh:TripleRule;
    sh:subject sh:this; sh:predicate <urn:p>; sh:object 2]."""
    data = native_dataset("<urn:s> <urn:q> 1.")
    before = data.dump("nquads")
    with pytest.raises(ValueError, match="positive"):
        SharedShapes(source, True, [], _policy=_native_shacl._REGEX_POLICY).validate(
            data, '{"max_rule_iterations":0}', True
        )
    assert data.dump("nquads") == before
    with pytest.raises(RuntimeError, match="iteration limit"):
        SharedShapes(source, True, [], _policy=_native_shacl._REGEX_POLICY).validate(
            data, '{"iterate_rules":true,"max_rule_iterations":1}', True
        )
    assert data.dump("nquads") == before


def test_shared_program_is_reusable_across_concurrent_dataset_owners():
    from concurrent.futures import ThreadPoolExecutor

    program = SharedShapes(SHAPES, False, [], _policy=_native_shacl._REGEX_POLICY)
    valid = native_dataset("<urn:example:s> <urn:example:p> 1.")
    invalid = native_dataset("<urn:example:s> <urn:example:q> 1.")
    with ThreadPoolExecutor(max_workers=4) as pool:
        results = list(
            pool.map(
                lambda i: program.validate((valid if i % 2 else invalid), "{}", False)[0],
                range(20),
            )
        )
    assert results == [bool(i % 2) for i in range(20)]


def test_unknown_settings_and_invalid_focus_iris_are_hard_errors():
    data = NativeDataset()
    program = SharedShapes(SHAPES, False, [], _policy=_native_shacl._REGEX_POLICY)
    for settings in ('{"focus_nodes":["not an IRI"]}', '{"max_rule_iterations":-1}', "[]"):
        with pytest.raises(ValueError):
            program.validate(data, settings, False)


def test_python_test_companion_allows_the_next_minor_release():
    root = Path(__file__).resolve().parents[1]
    project = tomllib.loads((root / "pyproject.toml").read_text())
    requirements = map(Requirement, project["project"]["optional-dependencies"]["test"])
    companion = next(item for item in requirements if item.name == "pyshacl-turbo")
    assert "0.6.0" in companion.specifier
