"""Graph metadata reads must observe the transaction's own commit boundary."""

import pytest
from rdflib import BNode, Literal, URIRef
from rdflib.graph import DATASET_DEFAULT_GRAPH_ID
from rdflib_turbo import StoreDataset

TRIPLE = (URIRef("urn:s"), URIRef("urn:p"), Literal("value"))
OLD, NEW = URIRef("urn:old"), BNode("new")


@pytest.mark.parametrize("commit", [False, True])
def test_context_names_counts_and_removal_share_transaction_state(commit):
    dataset = StoreDataset()
    dataset.graph(OLD).add(TRIPLE)
    try:
        with dataset.transaction() as transaction:
            transaction.remove_graph(OLD)
            named = transaction.graph(NEW)
            named.add(TRIPLE)
            transaction.default_graph.add(TRIPLE)
            assert set(transaction.store.contexts()) == {NEW, DATASET_DEFAULT_GRAPH_ID}
            assert len(named) == len(transaction.default_graph) == len(transaction) == 1
            assert set(dataset.store.contexts()) == {OLD}
            assert len(dataset.graph(OLD)) == 1
            assert bool(named.query('ASK { <urn:s> <urn:p> "value" }'))
            native = transaction.store.backend._native
            assert native.count_pattern(transaction.store._pattern(TRIPLE, named)) == 1
            assert native.count_pattern(transaction.store._pattern(TRIPLE, None)) == 1
            transaction.remove_graph(transaction.default_graph)
            assert len(transaction.default_graph) == 0
            assert set(transaction.store.contexts()) == {NEW}
            if not commit:
                raise RuntimeError("abort metadata")
    except RuntimeError as error:
        assert not commit
        assert str(error) == "abort metadata"
    expected = NEW if commit else OLD
    assert set(dataset.store.contexts()) == {expected}
    assert len(dataset.graph(expected)) == 1
    assert len(dataset.default_graph) == 0
    assert len(dataset) == 1
    dataset.close()
