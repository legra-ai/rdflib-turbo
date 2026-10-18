"""Exact membership asks the native index for a boolean without transporting rows."""

from types import MethodType

import pytest
from rdflib import BNode, Literal, URIRef
from rdflib.namespace import XSD
from rdflib_turbo import Graph, StoreDataset
from rdflib_turbo._reads import encode

SUBJECT, PREDICATE, OBJECT = URIRef("relative/subject"), URIRef("urn:p"), Literal("value")
TRIPLE = (SUBJECT, PREDICATE, OBJECT)
ABSENT = (URIRef("urn:absent"), PREDICATE, OBJECT)


def forbid(*args, **kwargs):
    raise AssertionError("exact membership entered row transport")


def test_exact_presence_and_absence_never_enter_row_transport(monkeypatch):
    graph = Graph().add(TRIPLE)
    original = graph.store.backend._native
    calls = []

    class BooleanReads:
        read_pattern = staticmethod(forbid)

        def contains_terms(self, *args):
            calls.append(args)
            return original.contains_terms(*args)

    monkeypatch.setattr(graph.store.backend, "_native", BooleanReads())
    assert TRIPLE in graph
    assert ABSENT not in graph
    assert len(calls) == 2
    assert calls[0] == (*map(encode, TRIPLE), None)


@pytest.mark.parametrize("identifier", [URIRef("urn:g"), URIRef("relative/g"), BNode("g")])
def test_exact_boolean_preserves_default_and_named_scope(identifier):
    dataset = StoreDataset()
    named = dataset.graph(identifier).add(TRIPLE)
    assert TRIPLE in named
    assert TRIPLE not in dataset.default_graph
    dataset.default_graph.add(ABSENT)
    assert ABSENT in dataset.default_graph
    assert ABSENT not in named
    dataset.default_graph.add(TRIPLE)
    assert TRIPLE in named and TRIPLE in dataset.default_graph
    standalone = Graph(identifier=BNode("default")).add(TRIPLE)
    assert TRIPLE in standalone


@pytest.mark.parametrize(
    "stored,other",
    [
        (Literal("value"), Literal("value", datatype=XSD.string, normalize=False)),
        (Literal("01", datatype=XSD.integer, normalize=False), Literal("1", datatype=XSD.integer)),
        (Literal("value", lang="en"), Literal("value", lang="fr")),
        (BNode("label"), BNode("another")),
    ],
)
def test_boolean_membership_retains_exact_object_identity(stored, other):
    graph = Graph().add((SUBJECT, PREDICATE, stored))
    assert (SUBJECT, PREDICATE, stored) in graph
    assert (SUBJECT, PREDICATE, other) not in graph
    assert type(graph.__contains__((SUBJECT, PREDICATE, stored))) is bool


def test_membership_preserves_subclass_virtual_and_filtered_triples():
    class Filtered(Graph):
        def triples(self, pattern):
            yield from ()

    class Virtual(Graph):
        def triples(self, pattern):
            assert pattern is marker
            yield TRIPLE

    assert TRIPLE not in Filtered().add(TRIPLE)
    marker = object()
    assert marker in Virtual()


def test_membership_preserves_instance_triples_override(monkeypatch):
    graph = Graph().add(TRIPLE)
    calls = []

    def virtual(self, pattern):
        calls.append(pattern)
        if pattern == ABSENT:
            yield ABSENT

    monkeypatch.setattr(graph, "triples", MethodType(virtual, graph))
    assert ABSENT in graph
    assert TRIPLE not in graph
    assert calls == [ABSENT, TRIPLE]


@pytest.mark.parametrize("mask", range(7))
def test_partial_patterns_keep_existing_generator_contract(monkeypatch, mask):
    graph = Graph().add(TRIPLE)
    native = graph.store.backend._native

    class RowsOnly:
        contains_terms = staticmethod(forbid)

        def read_pattern(self, *args):
            return native.read_pattern(*args)

    monkeypatch.setattr(graph.store.backend, "_native", RowsOnly())
    pattern = tuple(term if mask & (1 << index) else None for index, term in enumerate(TRIPLE))
    assert pattern in graph
    if mask & 1:
        assert (URIRef("urn:missing"), pattern[1], pattern[2]) not in graph


def test_path_membership_keeps_existing_path_evaluation(monkeypatch):
    graph = Graph().add((SUBJECT, PREDICATE, URIRef("urn:middle")))
    graph.add((URIRef("urn:middle"), URIRef("urn:q"), OBJECT))
    native = graph.store.backend._native

    class RowsOnly:
        contains_terms = staticmethod(forbid)

        def read_pattern(self, *args):
            return native.read_pattern(*args)

    monkeypatch.setattr(graph.store.backend, "_native", RowsOnly())
    assert (SUBJECT, PREDICATE / URIRef("urn:q"), OBJECT) in graph
    assert (SUBJECT, PREDICATE / URIRef("urn:missing"), OBJECT) not in graph


def test_transaction_membership_uses_owner_and_preserves_commit_and_abort(monkeypatch):
    dataset = StoreDataset()
    dataset.default_graph.add(TRIPLE)
    with dataset.transaction() as view:
        native = view.store.backend._native

        class OwnerOnly:
            contains_terms = staticmethod(forbid)

            def __getattr__(self, name):
                return getattr(native, name)

        monkeypatch.setattr(view.store.backend, "_native", OwnerOnly())
        assert TRIPLE in view.default_graph
        view.default_graph.remove(TRIPLE).add(ABSENT)
        assert TRIPLE not in view.default_graph and ABSENT in view.default_graph
        assert TRIPLE in dataset.default_graph and ABSENT not in dataset.default_graph
    assert ABSENT in dataset.default_graph and TRIPLE not in dataset.default_graph
    with pytest.raises(ValueError, match="abort"):
        with dataset.transaction() as view:
            view.default_graph.add(TRIPLE)
            raise ValueError("abort")
    assert TRIPLE not in dataset.default_graph
    with pytest.raises(RuntimeError, match="closed"):
        _ = ABSENT in view.default_graph


@pytest.mark.parametrize("slot", range(4))
def test_invalid_boolean_input_is_read_error_not_false_or_writer_reservation(slot):
    graph = Graph().add(TRIPLE)
    args = [*map(encode, TRIPLE), None]
    args[slot] = ("unknown", "x", None, None)
    with pytest.raises(ValueError):
        graph.store.backend._native.contains_terms(*args)
    assert TRIPLE in graph
    graph.add(ABSENT)
    graph.close()
    with pytest.raises(RuntimeError, match="closed"):
        _ = TRIPLE in graph


def test_dataset_membership_retains_triple_and_quad_semantics():
    from rdflib import Dataset as ReferenceDataset

    def observe(factory):
        dataset = factory(default_union=True)
        named = dataset.graph(URIRef("urn:g")).add(TRIPLE)
        other = dataset.graph(URIRef("urn:other")).add(ABSENT)
        result = [
            TRIPLE in dataset,
            (*TRIPLE, named) in dataset,
            (*TRIPLE, other) in dataset,
            (*TRIPLE, URIRef("urn:missing")) in dataset,
            TRIPLE in dataset.default_graph,
        ]
        dataset.default_union = False
        result.extend([TRIPLE in dataset, (*TRIPLE, named) in dataset])
        return result

    # Empty-context truthiness is inherited behavior; preserve reference semantics.
    assert observe(StoreDataset) == observe(ReferenceDataset)


def test_custom_attribute_dispatch_keeps_virtual_triples():
    class Dynamic(Graph):
        def __getattribute__(self, name):
            if name == "triples":
                return lambda pattern: iter((TRIPLE,))
            return super().__getattribute__(name)

    assert object() in Dynamic()


def test_boolean_membership_accepts_list_pattern_and_case_insensitive_language():
    graph = Graph().add((SUBJECT, PREDICATE, Literal("value", lang="EN")))
    assert [SUBJECT, PREDICATE, Literal("value", lang="en")] in graph


@pytest.mark.parametrize(
    "slot,invalid",
    [
        (0, ("literal", "x", None, None)),
        (1, ("bnode", "p", None, None)),
        (2, ("literal", "x", "urn:datatype", "en")),
        (3, ("literal", "x", None, None)),
        (0, ("uri", "x", "urn:datatype", None)),
    ],
)
def test_boolean_membership_validates_term_positions(slot, invalid):
    graph = Graph().add(TRIPLE)
    args = [*map(encode, TRIPLE), None]
    args[slot] = invalid
    with pytest.raises(ValueError):
        graph.store.backend._native.contains_terms(*args)
    assert TRIPLE in graph


def test_failed_native_boolean_read_propagates_without_row_retry(monkeypatch):
    graph = Graph().add(TRIPLE)

    class BrokenRead:
        read_pattern = staticmethod(forbid)

        def contains_terms(self, *args):
            raise RuntimeError("lookup failed")

    monkeypatch.setattr(graph.store.backend, "_native", BrokenRead())
    with pytest.raises(RuntimeError, match="lookup failed"):
        _ = TRIPLE in graph


def test_membership_semantic_error_still_aborts_explicit_owner():
    graph = Graph().add(TRIPLE)
    with pytest.raises(RuntimeError, match="closed"):
        with graph.transaction() as transaction:
            transaction.add(ABSENT)
            with pytest.raises(ValueError):
                _ = (Literal("invalid subject"), PREDICATE, OBJECT) in transaction
    assert ABSENT not in graph
