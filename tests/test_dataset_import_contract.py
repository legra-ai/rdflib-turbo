"""Explicit native Dataset import preserves full terms and logical graph domains."""

import pytest
from rdflib import BNode, Graph, Literal, Namespace
from rdflib import Dataset as RDFLibDataset
from rdflib.namespace import XSD
from rdflib_turbo import StoreDataset

EX = Namespace("urn:dataset-import:")


def key(term):
    return (
        type(term).__name__,
        str(term),
        str(term.datatype) if isinstance(term, Literal) and term.datatype else None,
        term.language if isinstance(term, Literal) else None,
    )


def observations(dataset):
    return {
        "rows": {tuple(key(term) for term in quad) for quad in dataset.quads((None,) * 4)},
        "graphs": {key(graph.identifier) for graph in dataset.graphs()},
        "namespaces": dict(dataset.namespaces()),
        "default_union": dataset.default_union,
    }


@pytest.mark.parametrize("generalized", (False, True))
def test_dataset_import_and_explicit_export_preserve_terms_domains_and_namespaces(
    generalized, monkeypatch
):
    source = RDFLibDataset(default_union=True)
    source.bind("ex", EX)
    source.bind("rdf", "urn:explicit-import-rdf:", replace=True)
    shared = BNode("shared-source-node")
    source.default_graph.add((shared, EX.p, Literal("plain")))
    source.graph(EX.named).add((EX.s, EX.p, shared))
    source.graph(BNode("blank-domain")).add(
        (shared, EX.q, Literal("+001", datatype=XSD.integer, normalize=False))
    )
    source.graph(EX.empty)
    source.graph(BNode("empty-blank-domain"))
    if generalized:
        source.default_graph.add((Literal("literal subject"), Literal("literal predicate"), shared))
    before = observations(source)

    def no_serialization(*args, **kwargs):
        raise AssertionError("Typed Dataset import must not serialize its source")

    monkeypatch.setattr(source, "serialize", no_serialization)
    native = StoreDataset.from_rdflib(source)
    assert isinstance(native, StoreDataset)
    assert observations(native) == before
    exported = native.to_rdflib()
    assert type(exported) is RDFLibDataset
    assert observations(exported) == before
    assert observations(source) == before
    native.default_graph.add((EX.changed, EX.p, Literal("native")))
    exported.bind("export-only", "urn:export-only:")
    assert observations(source) == before
    assert (EX.changed, EX.p, None) not in exported.default_graph
    assert "export-only" not in dict(native.namespaces())


def test_dataset_import_plain_graph_maps_only_its_rows_into_logical_default(monkeypatch):
    source = Graph(identifier=EX.source)
    source.bind("ex", EX)
    source.add((EX.s, EX.p, Literal("value")))

    def no_serialization(*args, **kwargs):
        raise AssertionError("Graph import must not serialize its source")

    monkeypatch.setattr(source, "serialize", no_serialization)
    native = StoreDataset.from_rdflib(source)
    assert set(native.default_graph) == set(source)
    assert {g.identifier for g in native.graphs()} == {native.default_graph.identifier}
    assert dict(native.namespaces()) == dict(source.namespaces())


def test_explicit_reference_import_rejects_native_owner_before_any_transport(monkeypatch):
    source = StoreDataset()

    def no_transport(*args, **kwargs):
        raise AssertionError("Existing native Dataset must not be reimported")

    monkeypatch.setattr(source, "serialize", no_transport)
    monkeypatch.setattr(source, "quads", no_transport)
    with pytest.raises(NotImplementedError):
        StoreDataset.from_rdflib(source)
