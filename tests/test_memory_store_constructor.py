"""RDFLib's explicit Memory spelling selects the existing native memory owner."""

import pytest
from rdflib import Dataset as ReferenceDataset
from rdflib import Graph as ReferenceGraph
from rdflib import Literal, URIRef
from rdflib_turbo.graph import Graph, StoreDataset


@pytest.mark.parametrize("store", ["default", "Memory"])
@pytest.mark.parametrize(
    "native, reference", [(Graph, ReferenceGraph), (StoreDataset, ReferenceDataset)]
)
def test_explicit_memory_store_preserves_normal_constructor_behavior(store, native, reference):
    expected, actual = reference(store=store), native(store=store)
    triple = URIRef("urn:subject"), URIRef("urn:predicate"), Literal("value")
    for graph in (expected, actual):
        target = graph.default_graph if isinstance(graph, ReferenceDataset) else graph
        target.add(triple)
        assert set(target) == {triple}
    assert actual.store.backend.__class__.__module__ == "rdflib_turbo.store"
    independent = native(store=store)
    assert actual.store is not independent.store
    assert len(independent) == 0
