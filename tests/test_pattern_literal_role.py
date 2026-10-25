"""Ordinary string and language pattern terms retain complete reference reports."""

import pyshacl
import pyshacl_turbo
import pytest
from rdflib import Graph, Literal, Namespace
from rdflib.compare import to_canonical_graph
from rdflib.namespace import RDF, SH, XSD

EX = Namespace("urn:pattern-literal-role:")


def observation(api, role, focused):
    data, shapes = Graph(), Graph()
    for value in ("abc", "no"):
        data.add((EX.focus, EX.p, Literal(value)))
    pattern = {
        "plain": Literal("abc"),
        "language": Literal("abc", lang="en"),
        "string": Literal("abc", datatype=XSD.string),
    }[role]
    shapes.add((EX.shape, RDF.type, SH.PropertyShape))
    shapes.add((EX.shape, SH.path, EX.p))
    shapes.add((EX.shape, SH.pattern, pattern))
    if focused:
        shapes.add((EX.shape, SH.targetNode, EX.focus))
    before = set(data)
    conforms, report, text = api.validate(data, shacl_graph=shapes)
    assert set(data) == before
    return (conforms, set(to_canonical_graph(report)), text), (data, shapes, report)


@pytest.mark.parametrize("role", ("plain", "language", "string"))
@pytest.mark.parametrize("focused", (False, True))
def test_pattern_literal_text_roles_match_original_complete_report(role, focused):
    expected, original_owners = observation(pyshacl, role, focused)
    assert expected[0] is (not focused)
    actual, turbo_owners = observation(pyshacl_turbo, role, focused)
    assert actual == expected
    assert original_owners[0] is not turbo_owners[0]
