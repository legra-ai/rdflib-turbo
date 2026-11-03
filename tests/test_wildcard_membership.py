"""Wildcard Graph membership returns one boolean without RDF result transport."""

import pytest
from rdflib import Literal, URIRef
from rdflib_turbo import Graph, _reads
from rdflib_turbo._literal import restore
from rdflib_turbo._native import NativeTermFactories

SUBJECT = URIRef("urn:membership:subject")
PREDICATE = URIRef("urn:membership:predicate")
TRIPLE = (SUBJECT, PREDICATE, Literal(0))


def forbid_rows(*args, **kwargs):
    raise AssertionError("Boolean membership must not enter result-row transport")


@pytest.mark.parametrize("mask", range(7))
def test_wildcard_presence_and_absence_use_native_boolean_query(monkeypatch, mask):
    graph = Graph().add(TRIPLE)
    native = graph.store.backend._native
    calls = []

    class BooleanReads:
        read_pattern = staticmethod(forbid_rows)
        read_triples = staticmethod(forbid_rows)
        read_quads = staticmethod(forbid_rows)

        def contains_pattern(self, pattern, context, inputs):
            calls.append((pattern, context))
            return native.contains_pattern(pattern, context, inputs)

    monkeypatch.setattr(graph.store.backend, "_native", BooleanReads())
    pattern = tuple(term if mask & (1 << index) else None for index, term in enumerate(TRIPLE))
    assert graph.__contains__(pattern) is True
    assert calls == [(pattern, graph)]
    if mask:
        absent = list(pattern)
        index = next(index for index, term in enumerate(pattern) if term is not None)
        absent[index] = URIRef("urn:membership:missing")
        assert graph.__contains__(absent) is False
        assert calls[-1][0] is absent


def test_wildcard_membership_does_not_construct_unused_literals(monkeypatch):
    graph = Graph()
    for index in range(256):
        graph.add((SUBJECT, PREDICATE, Literal(index)))
    objects = []

    def literal(*args):
        objects.append(args)
        return restore(*args)

    monkeypatch.setattr(
        _reads,
        "_TERM_FACTORIES",
        NativeTermFactories(URIRef, _reads.BNode, literal, str.__new__, True),
    )
    assert (SUBJECT, PREDICATE, None) in graph
    assert objects == []
