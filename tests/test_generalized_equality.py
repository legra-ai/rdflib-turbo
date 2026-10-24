"""Equality can pass through non-RDF tuples and return to ordinary RDF."""

import pytest
from inference_observers import physical_keys, union_graph
from owlrl import DeductiveClosure, OWLRL_Semantics
from rdflib import BNode, Graph, URIRef
from rdflib.compare import isomorphic
from rdflib_turbo import Dataset, Store
from rdflib_turbo import Graph as NativeGraph

PREFIX = "@prefix owl: <http://www.w3.org/2002/07/owl#> . "
CASES = [
    (
        '<urn:a> owl:sameAs "shared" . <urn:b> owl:sameAs "shared" . <urn:a> <urn:p> <urn:value> .',
        (URIRef("urn:b"), URIRef("urn:p"), URIRef("urn:value")),
    ),
    (
        '<urn:a> <urn:p> "shared" . <urn:b> owl:sameAs "shared" .',
        (URIRef("urn:a"), URIRef("urn:p"), URIRef("urn:b")),
    ),
    (
        '<urn:p> owl:sameAs "shared" . <urn:q> owl:sameAs "shared" . <urn:a> <urn:p> <urn:value> .',
        (URIRef("urn:a"), URIRef("urn:q"), URIRef("urn:value")),
    ),
]


@pytest.mark.parametrize("document,consequence", CASES)
@pytest.mark.parametrize("api", ["dataset", "store", "transaction", "persistent"])
def test_generalized_equality_matches_reference(document, consequence, api, tmp_path):
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
                assert consequence in actual
                assert isomorphic(actual, expected)


@pytest.mark.parametrize("identifier", [URIRef("urn:source"), BNode("source")])
@pytest.mark.parametrize("inside", [False, True])
def test_equality_uses_only_the_selected_source_graph(identifier, inside):
    with Store() as store:
        source = NativeGraph(store, identifier, bind_namespaces="none")
        # A caller graph with the internal query-view IRI remains ordinary data.
        outside = NativeGraph(
            store, URIRef("urn:rdflib-turbo:inference:generalized"), bind_namespaces="none"
        )
        source.parse(data='<urn:a> <urn:p> "shared" .', format="turtle")
        alias = '<urn:b> <http://www.w3.org/2002/07/owl#sameAs> "shared" .'
        outside.parse(data=alias, format="turtle")
        if inside:
            source.parse(data=alias, format="turtle")
        source.infer(inference="owlrl")
        assert ((URIRef("urn:a"), URIRef("urn:p"), URIRef("urn:b")) in source) is inside
        assert len(outside) == 1


def test_generalized_equality_rolls_back_and_can_be_retried():
    with Store() as store:
        store.parse(data=PREFIX + CASES[0][0])
        before = physical_keys(store)
        with pytest.raises(RuntimeError, match="abort"):
            with store.transaction() as transaction:
                transaction.infer(inference="owlrl")
                assert transaction.query("ASK { <urn:b> <urn:p> <urn:value> }").askAnswer
                raise RuntimeError("abort")
        assert physical_keys(store) == before
        store.infer(inference="owlrl")
        assert store.query("ASK { <urn:b> <urn:p> <urn:value> }").askAnswer


def test_rdfs_mode_does_not_enable_equality():
    data = Dataset().parse(data=PREFIX + CASES[1][0]).infer(inference="rdfs")
    assert not data.query("ASK { <urn:a> <urn:p> <urn:b> }").askAnswer
