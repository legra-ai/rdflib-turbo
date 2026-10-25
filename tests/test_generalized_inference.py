"""Ordinary RDF consequences must survive generalized intermediate tuples."""

import pytest
from owlrl import DeductiveClosure, OWLRL_Semantics
from rdflib import Graph, Literal, URIRef
from rdflib.compare import isomorphic
from rdflib_turbo import Dataset, Store

from .inference_observers import physical_keys, union_graph

DOCUMENT = """
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
<urn:a> <urn:p> "value" .
<urn:p> owl:inverseOf <urn:q> .
<urn:q> rdfs:subPropertyOf <urn:r> .
<urn:r> owl:inverseOf <urn:s> .
"""

BLANK_PREDICATE_DOCUMENT = """
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
<urn:a> <urn:p> "value" .
<urn:p> rdfs:subPropertyOf _:predicate .
_:predicate rdfs:subPropertyOf <urn:s> .
"""


@pytest.mark.parametrize("api", ["dataset", "store", "transaction", "persistent"])
@pytest.mark.parametrize("document", [DOCUMENT, BLANK_PREDICATE_DOCUMENT])
def test_generalized_intermediates_preserve_ordinary_consequences(api, document, tmp_path):
    reference = Graph().parse(data=document, format="turtle")
    DeductiveClosure(OWLRL_Semantics).expand(reference)
    consequence = (URIRef("urn:a"), URIRef("urn:s"), Literal("value"))
    assert consequence in reference

    def check(graph):
        graph.parse(data=document).infer(inference="owlrl")
        actual = union_graph(graph)
        assert consequence in actual
        ordinary = Graph()
        for triple in reference:
            ordinary.add(triple)
        assert isomorphic(actual, ordinary)

    if api == "dataset":
        check(Dataset())
    else:
        with Store(tmp_path / "store" if api == "persistent" else None) as store:
            if api == "transaction":
                with store.transaction() as transaction:
                    check(transaction)
            else:
                check(store)
        if api == "persistent":
            with Store(tmp_path / "store") as reopened:
                assert reopened.query('ASK { <urn:a> <urn:s> "value" }').askAnswer


def test_generalized_inference_respects_transaction_rollback_and_call_lifetime():
    with Store() as store:
        store.parse(data=DOCUMENT)
        before = physical_keys(store)
        with pytest.raises(RuntimeError, match="abort"):
            with store.transaction() as transaction:
                transaction.infer(inference="owlrl")
                assert transaction.query('ASK { <urn:a> <urn:s> "value" }').askAnswer
                raise RuntimeError("abort")
        assert physical_keys(store) == before
        # A new invocation owns a fresh intermediate relation after rollback.
        store.infer(inference="owlrl")
        assert store.query('ASK { <urn:a> <urn:s> "value" }').askAnswer


def test_generalized_inference_does_not_join_schema_from_another_source_graph():
    from rdflib_turbo import Graph as NativeGraph

    with Store() as store:
        left = NativeGraph(store, URIRef("urn:left"), bind_namespaces="none")
        right = NativeGraph(store, URIRef("urn:right"), bind_namespaces="none")
        left.parse(data=DOCUMENT.rsplit("<urn:r>", 1)[0], format="turtle")
        right.parse(
            data="<urn:r> <http://www.w3.org/2002/07/owl#inverseOf> <urn:s> .",
            format="turtle",
        )
        left.infer(inference="owlrl")
        assert (URIRef("urn:a"), URIRef("urn:s"), Literal("value")) not in left
        assert len(right) == 1
