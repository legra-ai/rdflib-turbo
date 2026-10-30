"""Bootstrap namespaces in the parse stage before document declarations."""

import pytest
from rdflib import Dataset as ReferenceDataset
from rdflib import Graph as ReferenceGraph
from rdflib import URIRef
from rdflib.namespace import NamespaceManager
from rdflib_turbo import Dataset, Graph

DOCUMENT = '@prefix rdfs:<http://www.w3.org/2000/01/rdf-schema#>. <urn:s> <urn:p> "value".'


def graphs(kind, mode):
    original, native = (ReferenceDataset, Dataset) if kind == "dataset" else (ReferenceGraph, Graph)
    options = {"bind_namespaces": mode} if mode in {"none", "core"} else {}
    return original(**options), native(**options)


@pytest.mark.parametrize(
    "kind,mode",
    [
        ("graph", "lazy"),
        ("dataset", "lazy"),
        ("graph", "preinitialized"),
        ("dataset", "preinitialized"),
        ("graph", "none"),
        ("graph", "core"),
        ("graph", "custom"),
        ("dataset", "custom"),
    ],
)
def test_successful_parse_preserves_original_namespace_bootstrap_order(kind, mode):
    original, native = graphs(kind, mode)
    for graph in (original, native):
        if mode == "preinitialized":
            tuple(graph.namespaces())
        elif mode == "custom":
            graph.namespace_manager = NamespaceManager(graph, bind_namespaces="none")
            graph.bind("custom", "urn:custom:")
        graph.parse(data=DOCUMENT, format="turtle")
    assert list(native.namespaces()) == list(original.namespaces())
    assert dict(native.namespaces())["rdfs"] == URIRef("http://www.w3.org/2000/01/rdf-schema#")
    if kind == "dataset":
        assert set(native.default_graph) == set(original.default_graph)
    else:
        assert set(native) == set(original)


@pytest.mark.parametrize("mode", ["rdflib", "core", "none"])
def test_failed_parse_does_not_publish_staged_bootstrap_or_change_owner(mode):
    graph = Graph(bind_namespaces=mode)
    owner = graph.store._namespaces
    before = list(owner.items()), owner.revision
    with pytest.raises(ValueError):
        graph.parse(data=DOCUMENT + "\n@prefix broken", format="turtle")
    assert graph.store._namespaces is owner
    assert (list(owner.items()), owner.revision) == before
    assert len(graph) == 0


def test_plain_transaction_does_not_bootstrap_namespaces():
    graph = Graph()
    owner = graph.store._namespaces
    with graph.transaction() as pending:
        pending.add((URIRef("urn:s"), URIRef("urn:p"), URIRef("urn:o")))
    assert graph.store._namespaces is owner
    assert list(owner.items()) == []
    assert len(graph) == 1


def test_parse_keeps_existing_custom_manager_and_does_not_add_callbacks():
    class Manager(NamespaceManager):
        def bind(self, *args, **kwargs):
            raise AssertionError("Parse must not introduce a caller-manager callback")

    graph = Graph()
    manager = Manager(graph, bind_namespaces="none")
    graph.namespace_manager = manager
    graph.parse(data=DOCUMENT, format="turtle")
    assert graph.namespace_manager is manager
    assert list(graph.namespaces()) == [("rdfs", URIRef("http://www.w3.org/2000/01/rdf-schema#"))]


@pytest.mark.parametrize(
    "kind,rdf_format,mode",
    [("graph", fmt, mode) for fmt in ("nt", "turtle", "ttl") for mode in ("lazy", "core", "none")]
    + [("dataset", fmt, "lazy") for fmt in ("nt", "turtle", "ttl", "trig")],
)
def test_no_declaration_parse_matches_original_store_bootstrap_before_public_access(
    kind, rdf_format, mode
):
    original, native = graphs(kind, mode)
    for graph in (original, native):
        assert list(graph.store.namespaces()) == []
        graph.parse(data='<urn:s> <urn:p> "value" .', format=rdf_format)
    # Inspect the public Store iterator first: graph.namespaces() would itself
    # initialize a lazy manager and conceal premature or missing bootstrap.
    assert list(native.store.namespaces()) == list(original.store.namespaces())
