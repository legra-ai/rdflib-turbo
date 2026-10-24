"""Inconsistencies are standard RDF output, distinct from execution failures."""

import pytest
from inference_observers import union_graph
from owlrl import DeductiveClosure, OWLRL_Semantics, RDFS_OWLRL_Semantics, RDFS_Semantics
from rdflib import RDF, Namespace, URIRef
from rdflib import Graph as ReferenceGraph
from rdflib.compare import isomorphic
from rdflib_turbo import Dataset, Store

ERR = Namespace("http://www.daml.org/2002/03/agents/agent-ont#")
PREFIXES = """
@prefix : <urn:diagnostic:> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
"""
CASES = {
    "disjoint-classes": ":A owl:disjointWith :B . :x a :A, :B .",
    "irreflexive-property": ":p a owl:IrreflexiveProperty . :x :p :x .",
    "asymmetric-property": ":p a owl:AsymmetricProperty . :x :p :y . :y :p :x .",
    "different-same": ":x owl:sameAs :y; owl:differentFrom :y .",
    "invalid-integer": ':x :p "not-an-integer"^^xsd:integer .',
    "disjoint-properties": ":p owl:propertyDisjointWith :q . :x :p :y; :q :y .",
    "nothing": ":x a owl:Nothing .",
    "complement": ":A owl:complementOf :B . :x a :A, :B .",
    "negative-object": ":assertion owl:sourceIndividual :x; owl:assertionProperty :p; owl:targetIndividual :y . :x :p :y .",
    "negative-data": ':assertion owl:sourceIndividual :x; owl:assertionProperty :p; owl:targetValue "value" . :x :p "value" .',
    "all-different": ":set a owl:AllDifferent; owl:members (:x :y :z) . :x owl:sameAs :y .",
    "all-different-final-excluded": ":set a owl:AllDifferent; owl:members (:x :y) . :x owl:sameAs :y .",
    "all-disjoint-properties": ":set a owl:AllDisjointProperties; owl:members (:p :q :r) . :x :p :y; :q :y .",
    "all-disjoint-properties-final-excluded": ":set a owl:AllDisjointProperties; owl:members (:p :q) . :x :p :y; :q :y .",
    "all-disjoint-classes": ":set a owl:AllDisjointClasses; owl:members (:A :B) . :x a :A, :B .",
    "max-cardinality-zero": ":R owl:onProperty :p; owl:maxCardinality 0 . :x a :R; :p :y .",
    "max-cardinality-false": ':R owl:onProperty :p; owl:maxCardinality "false"^^xsd:boolean . :x a :R; :p :y .',
    "max-cardinality-invalid-positive-zero": ':R owl:onProperty :p; owl:maxCardinality "0"^^xsd:positiveInteger . :x a :R; :p :y .',
    "max-qualified-zero": ":R owl:onProperty :p; owl:onClass :C; owl:maxQualifiedCardinality 0 . :x a :R; :p :y . :y a :C .",
    "max-qualified-thing-zero": ":R owl:onProperty :p; owl:onClass owl:Thing; owl:maxQualifiedCardinality 0 . :x a :R; :p :y .",
}
CLASSES = {"rdfs": RDFS_Semantics, "owlrl": OWLRL_Semantics, "both": RDFS_OWLRL_Semantics}


def messages(graph):
    return {str(message) for message in graph.objects(None, ERR.error)}


def native_graph(graph):
    return union_graph(graph)


@pytest.mark.parametrize("case", CASES)
@pytest.mark.parametrize("mode", CLASSES)
@pytest.mark.parametrize("api", ["dataset", "store", "transaction"])
def test_builtin_diagnostics_match_reference(case, mode, api):
    document = PREFIXES + CASES[case]
    expected = ReferenceGraph().parse(data=document, format="turtle")
    DeductiveClosure(CLASSES[mode]).expand(expected)
    if api == "dataset":
        graph = Dataset().parse(data=document).infer(inference=mode)
        actual = native_graph(graph)
    else:
        with Store() as store:
            if api == "transaction":
                with store.transaction() as transaction:
                    transaction.parse(data=document).infer(inference=mode)
            else:
                store.parse(data=document).infer(inference=mode)
            actual = native_graph(store)
    assert messages(actual) == messages(expected)
    if api == "dataset" and messages(expected):
        assert dict(graph.namespaces())["err"] == URIRef(ERR)
    assert isomorphic(actual, expected)
    # Every unique message gets its own fresh, unreasoned node after closure.
    for subject in actual.subjects(RDF.type, ERR.ErrorMessage):
        assert set(actual.predicate_objects(subject)) == {
            (RDF.type, ERR.ErrorMessage),
            (ERR.error, next(actual.objects(subject, ERR.error))),
        }


@pytest.mark.parametrize("mode", CLASSES)
@pytest.mark.parametrize("api", ["dataset", "store", "transaction"])
@pytest.mark.parametrize(
    "axiomatic,datatype", [(False, False), (True, False), (False, True), (True, True)]
)
def test_diagnostics_preserve_per_call_axiom_flags(mode, api, axiomatic, datatype):
    document = PREFIXES + CASES["nothing"]
    expected = ReferenceGraph().parse(data=document, format="turtle")
    DeductiveClosure(CLASSES[mode], axiomatic_triples=axiomatic, datatype_axioms=datatype).expand(
        expected
    )
    options = dict(inference=mode, axiomatic_triples=axiomatic, datatype_axioms=datatype)
    if api == "dataset":
        graph = Dataset().parse(data=document).infer(**options)
        actual = native_graph(graph)
    else:
        with Store() as store:
            if api == "transaction":
                with store.transaction() as transaction:
                    transaction.parse(data=document).infer(**options)
            else:
                store.parse(data=document).infer(**options)
            actual = native_graph(store)
    assert messages(actual) == messages(expected)
    assert isomorphic(actual, expected)
