"""Wildcard Graph membership returns one boolean without RDF result transport."""

import pytest
from rdflib import Graph as ReferenceGraph
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

        def matches_pattern(self, pattern, context, inputs):
            calls.append((pattern, context))
            return native.matches_pattern(pattern, context, inputs)

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


@pytest.mark.parametrize("base", [tuple, list])
@pytest.mark.parametrize("wildcard", [False, True])
def test_sequence_subclass_preserves_reference_iteration_protocol(monkeypatch, base, wildcard):
    graph = Graph().add(TRIPLE)
    native = graph.store.backend._native
    calls = []

    class Pattern(base):
        def __iter__(self):
            calls.append("iter")
            return super().__iter__()

    class RowsOnly:
        contains_pattern = staticmethod(forbid_rows)
        matches_pattern = staticmethod(forbid_rows)

        def read_pattern(self, *args):
            return native.read_pattern(*args)

        def read_union(self, *args, **kwargs):
            return native.read_union(*args, **kwargs)

    monkeypatch.setattr(graph.store.backend, "_native", RowsOnly())
    pattern = Pattern((SUBJECT, PREDICATE, None if wildcard else TRIPLE[2]))
    assert pattern in ReferenceGraph().add(TRIPLE)
    expected_calls = list(calls)
    calls.clear()
    assert pattern in graph
    assert calls == expected_calls == ["iter"]
