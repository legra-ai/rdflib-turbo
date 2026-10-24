"""Specialized list and cardinality rules retain generalized intermediates."""

import pytest
from inference_observers import physical_keys, union_graph
from owlrl import DeductiveClosure, OWLRL_Semantics
from rdflib import BNode, Graph, Literal, URIRef
from rdflib.compare import isomorphic
from rdflib_turbo import Dataset, Store
from rdflib_turbo import Graph as NativeGraph

PREFIX = """
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
"""
CASES = [
    (
        '<urn:a> <urn:p> "value" ; <urn:t> <urn:b> . <urn:p> owl:inverseOf <urn:q> . '
        "<urn:chain> owl:propertyChainAxiom (<urn:q> <urn:t>) ; owl:inverseOf <urn:back> .",
        (URIRef("urn:b"), URIRef("urn:back"), Literal("value")),
    ),
    (
        '<urn:a> <urn:p> "value" . <urn:p> owl:inverseOf <urn:q> . '
        "<urn:chain> owl:propertyChainAxiom (<urn:p> <urn:q>) .",
        (URIRef("urn:a"), URIRef("urn:chain"), URIRef("urn:a")),
    ),
    (
        '<urn:a> <urn:p> "value" . <urn:p> rdfs:range <urn:A>, <urn:B> . '
        "<urn:I> owl:intersectionOf (<urn:A> <urn:B>) . rdf:type owl:inverseOf <urn:typed> .",
        (URIRef("urn:I"), URIRef("urn:typed"), Literal("value")),
    ),
    (
        '<urn:a> <urn:p> "value" . <urn:p> rdfs:range <urn:C> . '
        '<urn:v> <urn:back> "value" . <urn:back> owl:inverseOf <urn:key> . '
        "<urn:b> a <urn:C> ; <urn:key> <urn:v> . <urn:C> owl:hasKey (<urn:key>) .",
        (URIRef("urn:a"), URIRef("urn:p"), URIRef("urn:b")),
    ),
    (
        "<urn:R> owl:onProperty <urn:p> ; owl:maxCardinality 1 . "
        '<urn:x> a <urn:R> ; <urn:p> "left", "right" . <urn:y> <urn:q> "left" .',
        (URIRef("urn:y"), URIRef("urn:q"), Literal("right")),
    ),
    (
        "<urn:R> owl:onProperty <urn:p> ; owl:maxQualifiedCardinality 1 ; owl:onClass <urn:C> . "
        "<urn:p> rdfs:range <urn:C> . "
        '<urn:x> a <urn:R> ; <urn:p> "left", "right" . <urn:y> <urn:q> "left" .',
        (URIRef("urn:y"), URIRef("urn:q"), Literal("right")),
    ),
    (
        "<urn:R> owl:onProperty <urn:p> ; owl:maxQualifiedCardinality 1 ; owl:onClass owl:Thing . "
        '<urn:x> a <urn:R> ; <urn:p> "left", "right" . <urn:y> <urn:q> "left" .',
        (URIRef("urn:y"), URIRef("urn:q"), Literal("right")),
    ),
]


@pytest.mark.parametrize("document,consequence", CASES)
@pytest.mark.parametrize("api", ["dataset", "store", "transaction", "persistent"])
def test_generalized_lists_match_reference(document, consequence, api, tmp_path):
    reference = Graph().parse(data=PREFIX + document, format="turtle")
    DeductiveClosure(OWLRL_Semantics).expand(reference)
    assert consequence in reference
    expected = Graph()
    for triple in reference:
        expected.add(triple)

    def check(graph):
        graph.parse(data=PREFIX + document).infer(inference="owlrl")
        actual = union_graph(graph)
        assert consequence in actual
        assert isomorphic(actual, expected)

    if api == "dataset":
        check(Dataset())
    else:
        path = tmp_path / "store" if api == "persistent" else None
        with Store(path) as store:
            if api == "transaction":
                with store.transaction() as transaction:
                    check(transaction)
            else:
                check(store)
        if api == "persistent":
            with Store(path) as reopened:
                actual = union_graph(reopened)
                assert isomorphic(actual, expected)


@pytest.mark.parametrize("identifier", [URIRef("urn:source"), BNode("source")])
@pytest.mark.parametrize("document,consequence", CASES)
def test_generalized_lists_preserve_source_scope(identifier, document, consequence):
    with Store() as store:
        source = NativeGraph(store, identifier, bind_namespaces="none")
        outside = NativeGraph(store, URIRef("urn:outside"), bind_namespaces="none")
        outside.parse(data=PREFIX + document, format="turtle")
        source.infer(inference="owlrl")
        assert consequence not in source
        source.parse(data=PREFIX + document, format="turtle")
        source.infer(inference="owlrl")
        assert consequence in source


@pytest.mark.parametrize("document,consequence", CASES)
def test_generalized_lists_rollback_and_retry(document, consequence):
    with Store() as store:
        store.parse(data=PREFIX + document)
        before = physical_keys(store)
        with pytest.raises(RuntimeError, match="abort"):
            with store.transaction() as transaction:
                transaction.infer(inference="owlrl")
                actual = union_graph(transaction)
                assert consequence in actual
                raise RuntimeError("abort")
        assert physical_keys(store) == before
        store.infer(inference="owlrl")
        actual = union_graph(store)
        assert consequence in actual
