"""Selected source descriptions include the actual blank-node closure."""

import pyshacl.rdfutil.stringify as reference
import pytest
from rdflib import BNode, Dataset, Graph, Literal, Namespace
from rdflib.collection import Collection
from rdflib.namespace import OWL, RDF
from rdflib_turbo import _literal

EX = Namespace("urn:source-description:")


def fixture(kind):
    graph = Dataset() if kind == "dataset" else Graph()
    graph.bind("ex", EX)
    target = graph.graph(EX.context) if kind == "dataset" else graph
    # The reference's process cache uses graph id and blank-node label. Fresh
    # labels keep independently allocated fixtures from sharing stale entries.
    root = BNode()
    if kind == "list":
        Collection(target, root, [Literal("z"), Literal("a"), EX.value])
    elif kind == "cycle":
        target.add((root, EX.next, root))
    elif kind == "same-as":
        target.add((root, OWL.sameAs, root))
    elif kind == "depth":
        parent = root
        for index in range(8):
            child = BNode(f"child-{index}")
            target.add((parent, EX.next, child))
            parent = child
    elif kind != "empty":
        target.add((root, RDF.type, EX.Class))
        target.add((root, EX.label, Literal('quoted "value"\nline')))
        target.add((root, EX.label, Literal("another")))
    return graph, root


@pytest.mark.parametrize(
    "kind", ["empty", "described", "list", "cycle", "same-as", "depth", "dataset"]
)
@pytest.mark.parametrize("name", ["_selected_stringify", "_selected_presentation"])
def test_selected_blank_description_matches_reference(kind, name):
    graph, root = fixture(kind)
    expected = reference.stringify_node(graph, root)
    assert getattr(_literal, name)(graph, root) == expected
