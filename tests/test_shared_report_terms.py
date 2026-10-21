"""Shared validation returns interned RDF terms without a report text boundary."""

import json

import pytest
from rdflib import BNode, Graph, Literal, URIRef
from rdflib.namespace import XSD
from rdflib_turbo import Dataset
from rdflib_turbo._native import SharedShapes

SHAPES = """@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
_:shape sh:targetSubjectsOf <urn:link>; sh:property _:property .
_:property sh:path _:path0; sh:in ("allowed");
    sh:message "plain", "typed"^^xsd:string, "message"@en .
_:path0 rdf:first <urn:link>; rdf:rest _:path1 .
_:path1 rdf:first <urn:p>; rdf:rest rdf:nil .
"""


def source_dataset():
    source = Graph()
    source.bind("ex", "urn:")
    source.add((BNode("focus"), URIRef("urn:link"), BNode("holder")))
    # bounded: four fixed literal identities, including distinct string forms.
    for value in (
        Literal("01", datatype=XSD.integer, normalize=False),
        Literal("word"),
        Literal("word", datatype=XSD.string),
        Literal("mot", lang="fr"),
    ):
        source.add((BNode("holder"), URIRef("urn:p"), value))
    return Dataset.from_rdflib(source)


def test_report_batch_preserves_exact_literals_and_blank_node_identities(monkeypatch):
    data = source_dataset()
    program = SharedShapes(SHAPES, False, [("ex", "urn:")])
    before = data.serialize()

    def forbidden(*args, **kwargs):
        raise AssertionError("Shared report transport must not parse or serialize RDF")

    with monkeypatch.context() as patch:
        patch.setattr(Graph, "parse", forbidden)
        patch.setattr(Graph, "serialize", forbidden)
        conforms, report, text, warnings = program.validate(data._native, "{}", False)

    assert isinstance(report, tuple), "Shared reports must use the existing RDF term batch"
    records, quads = report
    assert len(records) == len(set(records))
    expected = {
        (2, "01", str(XSD.integer), None),
        (2, "word", None, None),
        (2, "word", str(XSD.string), None),
        (2, "mot", None, "fr"),
        (2, "plain", None, None),
        (2, "typed", str(XSD.string), None),
        (2, "message", None, "en"),
        (1, "focus", None, None),
        (1, "property", None, None),
        (1, "path0", None, None),
    }
    assert expected <= set(records)
    assert all(0 <= index < len(records) for s, p, o, _ in quads for index in (s, p, o))
    assert all(graph is None for _, _, _, graph in quads)
    assert not conforms and "Results (4):" in text and "ex:link" in text
    assert not warnings
    assert data.serialize() == before


@pytest.mark.parametrize(
    "settings, conforms",
    [
        ({}, False),
        ({"allow_warnings": True}, False),
        ({"allow_infos": True}, False),
        ({"abort_on_first": True}, False),
        ({"focus_nodes": ["urn:other"]}, True),
        ({"use_shapes": ["urn:other"]}, True),
    ],
)
def test_report_batch_preserves_validation_options(settings, conforms):
    data = source_dataset()
    result = SharedShapes(SHAPES, False, []).validate(data._native, json.dumps(settings), False)
    assert result[0] is conforms
    assert isinstance(result[1], tuple)
