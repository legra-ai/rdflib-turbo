"""Rules execute on native Dataset ownership without a report or graph transport."""

import json
import os
import subprocess
import sys
import warnings
from pathlib import Path

import pyshacl_turbo as turbo
import pytest
from rdflib_turbo import Dataset
from rdflib_turbo._native import SharedShapes

SHAPES = """@prefix sh:<http://www.w3.org/ns/shacl#>.
@prefix ex:<urn:>.
ex:S sh:targetNode ex:a; sh:rule ex:R.
ex:R a sh:TripleRule; sh:subject sh:this;
  sh:predicate ex:derived; sh:object "01"^^<http://www.w3.org/2001/XMLSchema#integer>.
"""
SOURCE = "@prefix ex:<urn:>. ex:a ex:p ex:b."


def program(source=SHAPES):
    return SharedShapes(source, True, [("ex", "urn:")])


def wrap(native):
    data = Dataset()
    data._native = native
    return data


def snapshot(data):
    return data.serialize(format="nquads"), tuple(data.namespaces())


@pytest.mark.parametrize("inplace", [False, True])
def test_native_expansion_preserves_terms_namespaces_and_original_ownership(inplace):
    data = Dataset().parse(data=SOURCE)
    before = snapshot(data)
    native, messages = program().expand(data._native, "{}", inplace)
    assert messages == []
    if inplace:
        assert native is None
        result = data
    else:
        assert native is not None
        assert snapshot(data) == before
        result = wrap(native)
    assert tuple(result.namespaces()) == before[1]
    assert len(result) == 2
    literal = next(iter(result.query("SELECT ?o WHERE { <urn:a> <urn:derived> ?o }")))[0]
    assert str(literal) == "01"
    assert str(literal.datatype) == "http://www.w3.org/2001/XMLSchema#integer"
    assert bool(result.query("ASK { <urn:a> <urn:p> <urn:b> }"))


def test_native_expansion_retains_named_contexts_and_blank_identity():
    data = Dataset()
    data._native.load(
        '@prefix ex:<urn:>. ex:g { ex:a ex:p _:original. _:original ex:q "chat"@fr. }',
        "trig",
        None,
        True,
    )
    source = SHAPES.replace(
        'sh:object "01"^^<http://www.w3.org/2001/XMLSchema#integer>', "sh:object [sh:path ex:p]"
    )
    expanded, messages = program(source).expand(data._native, "{}", False)
    result = wrap(expanded)
    assert messages == []
    assert len(result) == 3
    assert bool(
        result.query(
            'ASK { GRAPH <urn:g> { <urn:a> <urn:p> ?blank. ?blank <urn:q> "chat"@fr } <urn:a> <urn:derived> ?blank }'
        )
    )
    assert not bool(data.query("ASK { <urn:a> <urn:derived> ?o }"))


def test_expansion_never_uses_python_graph_conversion_or_validates_unrelated_constraints(
    monkeypatch,
):
    data = Dataset().parse(data=SOURCE)
    compiled = program(SHAPES + "<urn:S> <http://www.w3.org/ns/shacl#node> <urn:S>.")

    def forbidden(*args, **kwargs):
        raise AssertionError("Native rule execution attempted Python graph transport")

    for name in ("copy", "serialize", "from_rdflib", "to_rdflib"):
        monkeypatch.setattr(Dataset, name, forbidden)
    with warnings.catch_warnings():
        warnings.simplefilter("error", turbo.ShapeRecursionWarning)
        native, messages = compiled.expand(data._native, "{}", True)
    assert native is None
    assert messages == []
    assert len(data) == 2


@pytest.mark.parametrize(
    "settings", [{"focus_nodes": ["urn:other"]}, {"use_shapes": ["urn:other"]}]
)
def test_rule_selectors_do_not_add_unselected_rules(settings):
    data = Dataset().parse(data=SOURCE)
    before = snapshot(data)
    result, warnings = program().expand(data._native, json.dumps(settings), True)
    assert result is None and warnings == []
    assert snapshot(data) == before


def test_sparql_rules_expand_repeatedly_without_duplicate_insertions():
    source = """@prefix sh:<http://www.w3.org/ns/shacl#>.
    <urn:S> sh:targetNode <urn:a>; sh:rule [a sh:SPARQLRule;
    sh:construct "CONSTRUCT { $this <urn:derived> ?o } WHERE { $this <urn:p> ?o }"] ."""
    data = Dataset().parse(data=SOURCE)
    compiled = program(source)
    assert compiled.expand(data._native, "{}", True) == (None, [])
    before = snapshot(data)
    assert compiled.expand(data._native, "{}", True) == (None, [])
    assert snapshot(data) == before
    assert len(data) == 2


@pytest.mark.parametrize(
    "settings",
    [
        '{"max_rule_iterations":0}',
        '{"iterate_rules":true,"max_rule_iterations":1}',
        '{"max_validation_depth":-1}',
        '{"iterate_rules":false,"iterate_rules":true}',
    ],
)
def test_failed_expansion_rolls_back_original_data_and_metadata(settings):
    data = Dataset().parse(data=SOURCE)
    before = snapshot(data)
    with pytest.raises((ValueError, RuntimeError)):
        program().expand(data._native, settings, True)
    assert snapshot(data) == before


def recursive_program():
    return program(
        SHAPES
        + "<urn:R> <http://www.w3.org/ns/shacl#condition> <urn:C>. <urn:C> <http://www.w3.org/ns/shacl#node> <urn:C>."
    )


@pytest.mark.parametrize("inplace", [False, True])
def test_warning_as_error_discards_staged_expansion(inplace):
    data = Dataset().parse(data=SOURCE)
    before = snapshot(data)
    with warnings.catch_warnings():
        warnings.simplefilter("error", turbo.ShapeRecursionWarning)
        with pytest.raises(turbo.ShapeRecursionWarning, match="Recursive Shape"):
            recursive_program().expand(data._native, "{}", inplace)
    assert snapshot(data) == before


def callback_write_case():
    data = Dataset().parse(data=SOURCE)
    observed = []

    def mutate(*args, **kwargs):
        assert len(data) == 1
        data.parse(data="<urn:callback> <urn:p> <urn:b>.")
        observed.append(snapshot(data))

    with warnings.catch_warnings():
        warnings.simplefilter("always", turbo.ShapeRecursionWarning)
        warnings.showwarning = mutate
        with pytest.raises(RuntimeError, match=r"Dataset changed.*warnings"):
            recursive_program().expand(data._native, "{}", True)
    assert snapshot(data) == observed[0]
    assert not bool(data.query("ASK { <urn:a> <urn:derived> ?o }"))


def test_warning_callback_write_conflicts_without_losing_intervening_writes(tmp_path):
    code = """import importlib.util, sys
spec = importlib.util.spec_from_file_location('shared_rules_case', sys.argv[1])
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
module.callback_write_case()
"""
    result = subprocess.run(
        [sys.executable, "-I", "-c", code, str(Path(__file__).resolve())],
        cwd=tmp_path,
        env=dict(os.environ),
        capture_output=True,
        text=True,
        timeout=15,
    )
    assert result.returncode == 0, result.stdout + result.stderr
