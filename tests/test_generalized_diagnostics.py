"""Diagnostic conditions read the same generalized closure as entailment rules."""

import pytest
from inference_observers import inference_outcome, physical_keys, union_graph
from owlrl import DeductiveClosure, OWLRL_Semantics, RDFS_OWLRL_Semantics
from rdflib import BNode, Graph, Namespace, URIRef
from rdflib.compare import isomorphic
from rdflib_turbo import Dataset, Store
from rdflib_turbo import Graph as NativeGraph

ERR = Namespace("http://www.daml.org/2002/03/agents/agent-ont#")
PREFIX = """
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
"""
CASES = {
    "disjoint-classes": '<urn:a> <urn:p> "value" . <urn:p> rdfs:range <urn:A>, <urn:B> . <urn:A> owl:disjointWith <urn:B> .',
    "nothing": '<urn:a> <urn:p> "value" . <urn:p> rdfs:range owl:Nothing .',
    "asymmetric": '<urn:a> <urn:p> "value" . <urn:p> a owl:AsymmetricProperty ; owl:inverseOf <urn:p> .',
    "irreflexive": '<urn:a> <urn:p> "value" . <urn:p> owl:inverseOf <urn:q> . <urn:q> owl:propertyChainAxiom (<urn:q> <urn:p>) ; a owl:IrreflexiveProperty .',
    "negative-assertion": '<urn:a> <urn:p> "value" . <urn:p> owl:inverseOf <urn:q> . <urn:assertion> owl:sourceIndividual "value" ; owl:assertionProperty <urn:q> ; owl:targetIndividual <urn:a> .',
    "disjoint-properties": '<urn:a> <urn:p> "value" . <urn:p> owl:inverseOf <urn:q>, <urn:r> . <urn:q> owl:propertyDisjointWith <urn:r> .',
    "complement": '<urn:a> <urn:p> "value" . <urn:p> rdfs:range <urn:A>, <urn:B> . <urn:A> owl:complementOf <urn:B> .',
    "all-disjoint-classes": '<urn:a> <urn:p> "value" . <urn:p> rdfs:range <urn:A>, <urn:B> . <urn:set> a owl:AllDisjointClasses ; owl:members (<urn:A> <urn:B>) .',
    "all-disjoint-properties": '<urn:a> <urn:p> "value" . <urn:p> owl:inverseOf <urn:q>, <urn:r> . <urn:set> a owl:AllDisjointProperties ; owl:members (<urn:q> <urn:r> <urn:unused>) .',
    "max-zero": '<urn:a> <urn:p> "value" . <urn:p> rdfs:range <urn:R> ; owl:inverseOf <urn:q> . <urn:R> owl:onProperty <urn:q> ; owl:maxCardinality 0 .',
    "max-qualified-zero": '<urn:a> <urn:p> "value" ; a <urn:C> . <urn:p> rdfs:range <urn:R> ; owl:inverseOf <urn:q> . <urn:R> owl:onProperty <urn:q> ; owl:onClass <urn:C> ; owl:maxQualifiedCardinality 0 .',
    "datatype-conflict": '<urn:a> <urn:p> "1"^^xsd:integer . <urn:b> <urn:q> "text"^^xsd:string . <urn:p> rdfs:range xsd:string .',
}


def messages(graph):
    return {str(message) for message in graph.objects(None, ERR.error)}


@pytest.mark.parametrize("case", CASES)
@pytest.mark.parametrize(
    "mode,semantics", [("owlrl", OWLRL_Semantics), ("both", RDFS_OWLRL_Semantics)]
)
@pytest.mark.parametrize("api", ["dataset", "store", "transaction", "persistent"])
def test_generalized_diagnostics_match_reference(case, mode, semantics, api, tmp_path):
    document = PREFIX + CASES[case]
    reference = Graph().parse(data=document, format="turtle")
    DeductiveClosure(semantics).expand(reference)
    assert messages(reference), "fixture must exercise an actual reference diagnostic"
    expected = Graph()
    for triple in reference:
        expected.add(triple)

    def check(graph):
        graph.parse(data=document).infer(inference=mode)
        actual = union_graph(graph)
        assert messages(actual) == messages(reference)
        assert set(inference_outcome(graph).error_messages) == messages(reference)
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
@pytest.mark.parametrize("inside", [False, True])
def test_generalized_diagnostics_respect_the_source_scope(identifier, inside):
    with Store() as store:
        source = NativeGraph(store, identifier, bind_namespaces="none")
        outside = NativeGraph(store, URIRef("urn:outside"), bind_namespaces="none")
        source.parse(
            data=PREFIX + '<urn:a> <urn:p> "value" . <urn:p> rdfs:range <urn:A>, <urn:B> .',
            format="turtle",
        )
        declaration = PREFIX + "<urn:A> owl:disjointWith <urn:B> ."
        outside.parse(data=declaration, format="turtle")
        if inside:
            source.parse(data=declaration, format="turtle")
        source.infer(inference="owlrl")
        actual = source
        assert bool(messages(actual)) is inside
        assert len(outside) == 1


@pytest.mark.parametrize("case", CASES)
def test_generalized_diagnostics_rollback_and_retry(case):
    with Store() as store:
        store.parse(data=PREFIX + CASES[case])
        before = physical_keys(store)
        with pytest.raises(RuntimeError, match="abort"):
            with store.transaction() as transaction:
                transaction.infer(inference="owlrl")
                assert inference_outcome(transaction).error_messages
                raise RuntimeError("abort")
        assert physical_keys(store) == before
        assert inference_outcome(store) is None
        store.infer(inference="owlrl")
        assert inference_outcome(store).error_messages
