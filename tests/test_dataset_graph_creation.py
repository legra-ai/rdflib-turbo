"""Ordinary Dataset graph construction retains RDFLib's context semantics."""

from rdflib import BNode, Graph, Literal, URIRef
from rdflib import Dataset as RDFLibDataset
from rdflib_turbo import Graph as NativeGraph
from rdflib_turbo import StoreDataset


def test_dataset_graph_without_identifier_uses_reference_skolem_policy():
    expected, actual = RDFLibDataset(), StoreDataset()
    original_graph, native_graph = expected.graph(), actual.graph()
    assert type(native_graph.identifier) is type(original_graph.identifier) is URIRef
    assert native_graph.identifier != BNode(str(native_graph.identifier))
    assert dict(actual.namespaces()) == dict(expected.namespaces())
    assert isinstance(native_graph, NativeGraph)
    assert native_graph.store is actual.store
    assert native_graph.identifier in {graph.identifier for graph in actual.graphs()}


def test_dataset_graph_imports_external_graph_into_its_own_store(monkeypatch):
    source = Graph(identifier=URIRef("urn:external-context"))
    source.add((BNode("shared"), URIRef("urn:p"), Literal("value")))
    source.add((URIRef("urn:s"), URIRef("urn:q"), BNode("shared")))
    expected, actual = RDFLibDataset(), StoreDataset()
    original_graph = expected.graph(source, base="urn:graph-base:")

    def forbid_serialization(*args, **kwargs):
        raise AssertionError("Dataset.graph(Graph) must use the native term boundary")

    monkeypatch.setattr(source, "serialize", forbid_serialization)
    native_graph = actual.graph(source, base="urn:graph-base:")
    assert isinstance(native_graph, NativeGraph)
    assert native_graph.store is actual.store
    assert native_graph.store is not source.store
    assert native_graph.identifier == original_graph.identifier == source.identifier
    assert native_graph.base == original_graph.base
    assert set(native_graph) == set(original_graph) == set(source)
    assert set(actual.quads((None,) * 4)) == set(expected.quads((None,) * 4))
