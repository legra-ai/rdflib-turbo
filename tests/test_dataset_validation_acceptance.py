"""Canonical Dataset rule output stays private until report acceptance."""

import pyshacl_turbo as turbo
import pytest
from rdflib import Graph
from rdflib_turbo import Dataset

SHAPES = """@prefix sh:<http://www.w3.org/ns/shacl#>.
<urn:S> sh:targetNode <urn:s>; sh:rule [a sh:TripleRule;
 sh:subject sh:this; sh:predicate <urn:derived>; sh:object true].
"""
SOURCE = "@prefix ex:<urn:>. ex:s ex:p ex:o."


def program():
    return turbo.compile_shapes(Graph().parse(data=SHAPES, format="turtle"), advanced=True)


def snapshot(data):
    return data.serialize(), tuple(data.namespaces())


@pytest.mark.parametrize("inferred", [False, True])
def test_report_decode_error_preserves_original_dataset_and_namespaces(monkeypatch, inferred):
    data = Dataset().parse(data=SOURCE)
    if inferred:
        data.infer(inference="rdfs")
    before = snapshot(data)
    compiled = program()

    def decode_failure(*args, **kwargs):
        raise ValueError("report decode failed")

    monkeypatch.setattr(turbo, "_graph", decode_failure)
    with pytest.raises(ValueError, match="report decode failed"):
        compiled.validate(data, inplace=True)
    assert snapshot(data) == before


@pytest.mark.parametrize("source", ["", "<urn:callback> <urn:p> 2."])
def test_report_callback_cannot_acquire_a_second_writer(monkeypatch, source):
    data = Dataset().parse(data=SOURCE)
    before = snapshot(data)
    compiled = program()
    decode = turbo._graph
    observed = []

    def callback(*args, **kwargs):
        assert not data.query("ASK {<urn:s> <urn:derived> true}")
        observed.append(snapshot(data))
        data.parse(data=source)
        return decode(*args, **kwargs)

    monkeypatch.setattr(turbo, "_graph", callback)
    with pytest.raises(RuntimeError, match="Store has an active transaction"):
        compiled.validate(data, inplace=True)
    assert observed == [before]
    assert snapshot(data) == before
    assert not data.query("ASK {<urn:s> <urn:derived> true}")
    # Report rejection releases the borrowed writer; a later writer succeeds.
    data.parse(data="<urn:after> <urn:p> 3.")
    assert data.query("ASK {<urn:after> <urn:p> 3}")
