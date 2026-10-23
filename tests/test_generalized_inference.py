"""Ordinary RDF consequences must survive generalized intermediate tuples."""

import pytest
from owlrl import DeductiveClosure, OWLRL_Semantics
from rdflib import Graph, Literal, URIRef
from rdflib_turbo import Dataset, Store

DOCUMENT = """
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
<urn:a> <urn:p> "value" .
<urn:p> owl:inverseOf <urn:q> .
<urn:q> rdfs:subPropertyOf <urn:r> .
<urn:r> owl:inverseOf <urn:s> .
"""


@pytest.mark.parametrize("api", ["dataset", "store", "transaction"])
def test_literal_subject_intermediates_preserve_ordinary_consequences(api):
    reference = Graph().parse(data=DOCUMENT, format="turtle")
    DeductiveClosure(OWLRL_Semantics).expand(reference)
    consequence = (URIRef("urn:a"), URIRef("urn:s"), Literal("value"))
    assert consequence in reference

    def check(graph):
        graph.parse(data=DOCUMENT).infer(inference="owlrl")
        actual = Graph().parse(data=graph.serialize(format="ntriples"), format="nt")
        assert consequence in actual
        assert all(not isinstance(subject, Literal) for subject, _, _ in actual)
        assert set(actual) == {triple for triple in reference if not isinstance(triple[0], Literal)}

    if api == "dataset":
        check(Dataset())
    else:
        with Store() as store:
            if api == "transaction":
                with store.transaction() as transaction:
                    check(transaction)
            else:
                check(store)
