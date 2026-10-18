"""Tiny metadata reads must not serialize and parse JSON patterns."""

from contextlib import nullcontext

import pytest
from rdflib import BNode, Literal, URIRef
from rdflib_turbo import StoreDataset


@pytest.mark.parametrize("transactional", [False, True])
@pytest.mark.parametrize("identifier", [URIRef("relative/graph"), BNode("graph")])
def test_graph_metadata_reads_use_typed_identity(monkeypatch, transactional, identifier):
    dataset = StoreDataset()
    triple = (URIRef("urn:s"), URIRef("urn:p"), Literal("value"))
    dataset.default_graph.add(triple)
    dataset.graph(identifier).add(triple)

    def forbidden(*args, **kwargs):
        raise AssertionError("metadata read serialized a JSON graph/pattern")

    with dataset.transaction() if transactional else nullcontext(dataset) as view:
        graph = view.graph(identifier)
        with monkeypatch.context() as patch:
            patch.setattr(view.store, "_pattern", forbidden)
            patch.setattr(view.store, "_graph", forbidden)
            assert len(view.default_graph) == len(graph) == 1
            assert view.graph(identifier).identifier == identifier
    dataset.close()


@pytest.mark.parametrize(
    "invalid",
    [
        ("unknown", "x", None, None),
        ("uri", "x", "urn:datatype", None),
        ("literal", "x", None, None),
    ],
)
def test_invalid_typed_graph_count_aborts_owner_but_not_committed_reader(invalid):
    dataset = StoreDataset()
    with pytest.raises(ValueError):
        dataset.store.backend._native.graph_len(invalid)
    with pytest.raises(RuntimeError, match=r"closed|finished"):
        with dataset.transaction() as transaction:
            transaction.graph(URIRef("urn:staged"))
            with pytest.raises(ValueError):
                transaction.store.backend._native.graph_len(invalid)
    assert set(dataset.store.contexts()) == set()
    assert dataset.store.backend._native.graph_len(None) == 0
    dataset.graph(URIRef("urn:next-writer"))
    dataset.close()
