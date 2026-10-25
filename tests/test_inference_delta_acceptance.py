"""Public delta decoding must finish before the caller's inference commits."""

import pytest
from rdflib import BNode, Literal, URIRef
from rdflib_turbo import StoreDataset
from rdflib_turbo import graph as facade


def populated():
    source = StoreDataset()
    source.default_graph.add(
        (
            URIRef("urn:Child"),
            URIRef("http://www.w3.org/2000/01/rdf-schema#subClassOf"),
            URIRef("urn:Parent"),
        )
    )
    source.graph(URIRef("urn:input")).add(
        (
            URIRef("urn:item"),
            URIRef("http://www.w3.org/1999/02/22-rdf-syntax-ns#type"),
            URIRef("urn:Child"),
        )
    )
    source.graph(BNode("empty"))
    source.default_graph.add((Literal("subject"), BNode("predicate"), Literal("object")))
    return source


def physical(source):
    return {
        tuple(
            (
                type(term).__name__,
                str(term),
                str(term.datatype)
                if isinstance(term, Literal) and term.datatype is not None
                else None,
                term.language if isinstance(term, Literal) else None,
            )
            for term in quad
        )
        for quad in source.quads((None,) * 4)
    }


def test_delta_materialization_error_aborts_caller_and_keeps_original_exception(monkeypatch):
    source = populated()
    assert callable(getattr(source, "infer_delta", None)), "Dataset requires atomic infer_delta"
    before = physical(source)
    domains = {context.identifier for context in source.graphs()}
    namespaces = dict(source.namespaces())
    failure = RuntimeError("injected delta materialization failure")
    original = facade._change_rows

    def fail_after_first_row(next_batch):
        rows = iter(original(next_batch))
        yield next(rows)
        raise failure

    monkeypatch.setattr(facade, "_change_rows", fail_after_first_row)
    with pytest.raises(RuntimeError) as caught:
        source.infer_delta(inference="rdfs")
    assert caught.value is failure
    assert physical(source) == before
    assert {context.identifier for context in source.graphs()} == domains
    assert dict(source.namespaces()) == namespaces


def test_delta_namespace_mutation_prevents_native_acceptance(monkeypatch):
    source = populated()
    assert callable(getattr(source, "infer_delta", None)), "Dataset requires atomic infer_delta"
    before = physical(source)
    original = facade._change_rows

    def mutate_metadata_before_accept(next_batch):
        yield from original(next_batch)
        source.bind("during", URIRef("urn:during:"))

    monkeypatch.setattr(facade, "_change_rows", mutate_metadata_before_accept)
    with pytest.raises(RuntimeError, match=r"[Nn]amespace"):
        source.infer_delta(inference="rdfs")
    assert physical(source) == before
    # The caller's own concurrent metadata edit is retained, never overwritten.
    assert dict(source.namespaces())["during"] == URIRef("urn:during:")
