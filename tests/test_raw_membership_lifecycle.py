"""Errors and finalizers at the raw Graph membership boundary."""

import weakref

import pytest
from rdflib import Literal, URIRef
from rdflib_turbo import Graph

TRIPLE = (URIRef("urn:s"), URIRef("urn:p"), Literal("value"))


def test_language_failure_releases_datatype_before_exception_propagates():
    graph = Graph()
    graph.add(TRIPLE)
    failure = ValueError("language getter failed")
    released = []
    references = []
    added = (URIRef("urn:finalizer"), TRIPLE[1], Literal("reentered"))

    class Datatype(URIRef):
        def __del__(self):
            graph.add(added)
            released.append(True)

    class Value(Literal):
        @property
        def datatype(self):
            value = Datatype("urn:datatype")
            references.append(weakref.ref(value))
            return value

        @property
        def language(self):
            raise failure

    with pytest.raises(ValueError) as caught:
        (TRIPLE[0], TRIPLE[1], Value("value")) in graph
    assert caught.value is failure
    # Tracebacks may own the property frame, but no staged metadata survives it.
    assert released == [True]
    assert references[0]() is None
    assert added in graph
    assert TRIPLE in graph


@pytest.mark.parametrize("stage", ["eq", "bool"])
@pytest.mark.parametrize("action", ["raise", "close"])
def test_graph_comparison_errors_and_close_preserve_boundary(stage, action):
    graph = None
    enabled = False
    failure = ValueError("comparison failed")
    events = []

    def callback(where):
        if not enabled:
            return
        events.append(where)
        if where == stage:
            if action == "raise":
                raise failure
            graph.store.backend.close()

    class Decision:
        def __bool__(self):
            callback("bool")
            return False

    class Identifier(URIRef):
        __hash__ = URIRef.__hash__

        def __eq__(self, other):
            callback("eq")
            return Decision()

    graph = Graph(identifier=Identifier("urn:named"))
    graph.add(TRIPLE)
    enabled = True
    if action == "raise":
        with pytest.raises(ValueError) as caught:
            TRIPLE in graph
        assert caught.value is failure
        enabled = False
        assert TRIPLE in graph
    else:
        with pytest.raises(RuntimeError, match="closed"):
            TRIPLE in graph
    assert events == (["eq"] if stage == "eq" and action == "raise" else ["eq", "bool"])


def test_metadata_replacing_descriptor_cannot_collect_active_classes(monkeypatch):
    import gc

    from rdflib import BNode
    from rdflib_turbo import _reads
    from rdflib_turbo._native import NativeInputTerms

    class UnusedBlank(BNode):
        pass

    reference = weakref.ref(UnusedBlank)
    descriptor = NativeInputTerms(URIRef, UnusedBlank, Literal)
    monkeypatch.setattr(_reads, "_INPUT_TERMS", descriptor)
    del descriptor, UnusedBlank
    graph = Graph()
    graph.add(TRIPLE)
    observed = []

    class Value(Literal):
        @property
        def datatype(self):
            _reads._INPUT_TERMS = NativeInputTerms(URIRef, BNode, Literal)
            gc.collect()
            observed.append(reference() is not None)
            return None

        @property
        def language(self):
            return None

    assert (TRIPLE[0], TRIPLE[1], Value("value")) in graph
    assert observed == [True]
    gc.collect()
    assert reference() is None
