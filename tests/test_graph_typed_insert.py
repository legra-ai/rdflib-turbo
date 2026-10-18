"""Insertion transports RDF terms without serializing a JSON document."""

import pytest
from rdflib import BNode, Literal, URIRef
from rdflib_turbo import Graph, StoreDataset


def test_add_never_serializes_json(monkeypatch):
    graph = Graph()
    import rdflib_turbo.graph as facade

    def forbidden(*args, **kwargs):
        raise AssertionError("Graph.add serialized JSON")

    monkeypatch.setattr(facade.json, "dumps", forbidden)
    values = [
        URIRef("relative:object"),
        BNode("caller-blank"),
        Literal("plain"),
        Literal("language", lang="en"),
        Literal("01", datatype=URIRef("http://www.w3.org/2001/XMLSchema#integer"), normalize=False),
    ]
    for value in values:
        graph.add((URIRef("urn:s"), URIRef("urn:p"), value))
    assert set(graph.objects()) == set(values)


def test_typed_insert_transaction_reads_uncommitted_values_and_aborts_invalid_terms():
    graph = Graph()
    triple = (URIRef("urn:s"), URIRef("urn:p"), Literal("value"))
    with graph.transaction() as transaction:
        transaction.add(triple)
        assert list(transaction) == [triple]
        assert not list(graph)
    assert list(graph) == [triple]
    with pytest.raises(RuntimeError, match="closed"):
        with graph.transaction() as transaction:
            transaction.add((URIRef("urn:discard"), triple[1], triple[2]))
            with pytest.raises(ValueError):
                transaction.store.backend._native.insert_terms(
                    ("literal", "invalid subject", None, None),
                    ("uri", "urn:p", None, None),
                    ("literal", "value", None, None),
                    None,
                )
    assert list(graph) == [triple]


def test_typed_add_preserves_named_graph_scope():
    dataset = StoreDataset()
    value = (BNode("s"), URIRef("urn:p"), Literal("value"))
    dataset.graph(URIRef("urn:named")).add(value)
    assert not list(dataset.default_graph)
    assert list(dataset.graph(URIRef("urn:named"))) == [value]
    with pytest.raises(ValueError, match="Graph context"):
        dataset.store.add(value, None)


def test_typed_add_and_legacy_selectors_agree_on_exact_literals():
    graph = Graph()
    values = [Literal("v", lang="en"), Literal("01", datatype=URIRef("urn:type"))]
    for value in values:
        graph.add((URIRef("urn:s"), URIRef("urn:p"), value))
    with graph.transaction() as transaction:
        for value in values:
            assert list(transaction.objects(URIRef("urn:s"), URIRef("urn:p"))).count(value) == 1
            transaction.remove((None, None, value))
        with pytest.raises(TypeError, match="Unsupported RDF term"):
            list(transaction.triples((42, None, None)))
        assert len(transaction) == 0
    assert len(graph) == 0
