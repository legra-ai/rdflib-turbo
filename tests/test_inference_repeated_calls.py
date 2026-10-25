"""Each OWL-RL invocation matches the original, including per-call datatype bootstrap."""

import json

import pytest
from owlrl import DeductiveClosure, OWLRL_Semantics
from rdflib import Graph as OriginalGraph
from rdflib_turbo import Dataset, Store
from rdflib_turbo._native import NativeDataset

from .inference_observers import physical_keys, term_key

SOURCE = """@prefix ex: <urn:example:>.
@prefix owl: <http://www.w3.org/2002/07/owl#>.
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#>.
ex:Employee rdfs:subClassOf ex:Person. ex:knows a owl:SymmetricProperty.
""" + "\n".join(
    f'ex:p{i} a ex:Employee; ex:knows ex:p{(i + 1) % 4}; ex:score {i}; ex:name "Person {i}".'
    for i in range(4)
)


def raw_physical_keys(owner):
    def key(term):
        return (
            {"uri": "URIRef", "bnode": "BNode", "literal": "Literal"}[term["type"]],
            term["value"],
            term.get("datatype"),
            term.get("xml:lang"),
        )

    return {
        (*map(key, row[:3]), None if row[3] is None else key(row[3]))
        for row in json.loads(owner.quads_json())
    }


@pytest.mark.parametrize("api", ["legacy", "dataset", "store"])
def test_each_repeated_closure_matches_complete_original_physical_relation(api):
    reference = OriginalGraph().parse(data=SOURCE, format="turtle")
    original = DeductiveClosure(
        OWLRL_Semantics, improved_datatypes=False, axiomatic_triples=False, datatype_axioms=False
    )
    if api == "legacy":
        owner = NativeDataset()
        owner.load(SOURCE, "turtle", None, True)
    else:
        owner = (Dataset() if api == "dataset" else Store()).parse(data=SOURCE, format="turtle")
    try:
        rounds = []
        for _ in range(2):
            original.expand(reference)
            if api == "legacy":
                owner.infer("owlrl", axiomatic_triples=False, datatype_axioms=False)
            else:
                owner.infer(inference="owlrl", axiomatic_triples=False, datatype_axioms=False)
            expected = {(*map(term_key, triple), None) for triple in reference}
            actual = raw_physical_keys(owner) if api == "legacy" else physical_keys(owner)
            assert actual == expected
            rounds.append(actual)
        expected_added = {
            (
                ("Literal", str(index), "http://www.w3.org/2001/XMLSchema#integer", None),
                ("URIRef", "http://www.w3.org/1999/02/22-rdf-syntax-ns#type", None, None),
                ("URIRef", "http://www.w3.org/2000/01/rdf-schema#Datatype", None, None),
                None,
            )
            for index in range(4)
        }
        assert rounds[0] <= rounds[1]
        assert rounds[1] - rounds[0] == expected_added
    finally:
        if api != "legacy":
            owner.close()
