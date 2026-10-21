"""Explicit native copies own their selected scope without Python RDF transport."""

import pytest
from rdflib import BNode, Literal, Namespace
from rdflib import Graph as ReferenceGraph
from rdflib.namespace import RDF, XSD
from rdflib_turbo import Dataset, Graph, Store, StoreDataset

EX = Namespace("urn:native-copy:")


def forbid(*args, **kwargs):
    raise AssertionError("native copy must not serialize or traverse Python triples")


@pytest.mark.parametrize("identifier", [None, EX.source, BNode("source")])
def test_graph_copy_selects_only_its_scope_and_owns_exact_terms(identifier, monkeypatch):
    source = Graph() if identifier is None else Graph(Store(), identifier)
    original = (BNode("subject"), EX.p, Literal("01", datatype=XSD.integer, normalize=False))
    source.add(original)
    outside = Graph(source.store, EX.outside)
    outside.add((EX.outside, RDF.type, EX.Outside))
    source.bind("example", EX)
    monkeypatch.setattr(Graph, "__iter__", forbid)
    monkeypatch.setattr(ReferenceGraph, "serialize", forbid)
    monkeypatch.setattr(Dataset, "from_rdflib", forbid)
    copied = source.copy()
    assert copied.identifier == source.identifier
    assert copied.store.backend is not source.store.backend
    assert copied.last_inference is None
    assert len(copied) == 1 and original in copied
    assert len(Graph(copied.store, EX.outside)) == 0
    assert dict(copied.namespaces()) == dict(source.namespaces())
    source.remove(original)
    source.store.backend.close()
    assert original in copied
    copied.add((EX.new, EX.p, EX.o))
    assert len(copied) == 2


@pytest.mark.parametrize("union", [False, True])
def test_dataset_copy_preserves_all_contexts_empty_graphs_and_namespace_ownership(
    union, monkeypatch
):
    source = StoreDataset(default_union=union)
    blank = BNode("graph")
    source.default_graph.add((EX.default, EX.p, EX.o))
    source.graph(blank).add((EX.blank, EX.p, Literal("text", lang="EN")))
    source.graph(EX.empty)
    source.bind("example", EX)
    monkeypatch.setattr(Graph, "__iter__", forbid)
    monkeypatch.setattr(ReferenceGraph, "serialize", forbid)
    copied = source.copy()
    assert copied.default_union is union
    assert copied.last_inference is None
    assert set(copied.quads()) == set(source.quads())
    assert {g.identifier for g in copied.graphs()} == {g.identifier for g in source.graphs()}
    assert dict(copied.namespaces()) == dict(source.namespaces())
    copied.bind("independent", EX.other)
    assert dict(copied.namespaces()) != dict(source.namespaces())
    source.store.backend.close()
    assert len(copied.graph(blank)) == 1


@pytest.mark.parametrize("dataset", [False, True])
def test_copy_reads_pending_transaction_view_without_publishing_to_source(dataset):
    original = StoreDataset() if dataset else Graph()
    if dataset:
        original.graph(EX.pending)
    failure = ValueError("rollback")
    with pytest.raises(ValueError) as caught:
        with original.transaction() as transaction:
            target = transaction.graph(EX.pending) if dataset else transaction
            target.add((EX.pending, EX.p, EX.o))
            transaction.bind("pending", EX)
            copied = transaction.copy()
            copied_target = copied.graph(EX.pending) if dataset else copied
            assert (EX.pending, EX.p, EX.o) in copied_target
            assert dict(copied.namespaces())["pending"] == EX
            assert len(original.graph(EX.pending) if dataset else original) == 0
            raise failure
    assert caught.value is failure
    assert len(original.graph(EX.pending) if dataset else original) == 0
    assert (EX.pending, EX.p, EX.o) in copied_target
    assert "pending" not in dict(original.namespaces())


def test_copy_of_closed_source_fails():
    graph = Graph()
    graph.store.backend.close()
    with pytest.raises(RuntimeError, match="closed"):
        graph.copy()
