"""Equality can pass through non-RDF tuples and return to ordinary RDF."""

import pytest
from owlrl import DeductiveClosure, OWLRL_Semantics
from rdflib import Graph, Literal, URIRef
from rdflib.compare import isomorphic
from rdflib_turbo import Dataset, Store

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
        if not isinstance(triple[0], Literal) and isinstance(triple[1], URIRef):
            expected.add(triple)

    def check(graph):
        graph.parse(data=PREFIX + document).infer(inference="owlrl")
        actual = Graph().parse(data=graph.serialize(format="ntriples"), format="nt")
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
                actual = Graph().parse(data=reopened.serialize(format="ntriples"), format="nt")
                assert consequence in actual
                assert isomorphic(actual, expected)
