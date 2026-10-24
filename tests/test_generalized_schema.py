"""Generalized schema terms keep their diagnostic meanings."""

import pytest
from owlrl import DeductiveClosure, OWLRL_Semantics
from rdflib import Graph, Literal, Namespace, URIRef
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
PREDICATES = {
    "irreflexive": '<urn:p> owl:sameAs "predicate" ; a owl:IrreflexiveProperty . <urn:x> <urn:p> <urn:x> .',
    "asymmetric": '<urn:p> owl:sameAs "predicate" ; a owl:AsymmetricProperty . <urn:x> <urn:p> <urn:y> . <urn:y> <urn:p> <urn:x> .',
    "disjoint": '<urn:p> owl:sameAs "left" ; owl:propertyDisjointWith <urn:q> . <urn:q> owl:sameAs "right" . <urn:x> <urn:p> <urn:y> ; <urn:q> <urn:y> .',
    "negative": '<urn:p> rdfs:subPropertyOf "predicate" . <urn:x> <urn:p> <urn:y> . <urn:assertion> owl:sourceIndividual <urn:x> ; owl:assertionProperty "predicate" ; owl:targetIndividual <urn:y> .',
    "max-zero": '<urn:p> rdfs:subPropertyOf "predicate" . <urn:x> a <urn:R> ; <urn:p> <urn:y> . <urn:R> owl:onProperty "predicate" ; owl:maxCardinality 0 .',
    "all-disjoint": '<urn:p> rdfs:subPropertyOf "left", "right" . <urn:x> <urn:p> <urn:y> . <urn:set> a owl:AllDisjointProperties ; owl:members ("left" "right" <urn:unused>) .',
}
BLANKS = {
    "irreflexive": "<urn:p> owl:sameAs _:predicate ; a owl:IrreflexiveProperty . <urn:x> <urn:p> <urn:x> .",
    "asymmetric": "<urn:p> owl:sameAs _:predicate ; a owl:AsymmetricProperty . <urn:x> <urn:p> <urn:y> . <urn:y> <urn:p> <urn:x> .",
    "disjoint": "<urn:p> owl:sameAs _:left ; owl:propertyDisjointWith <urn:q> . <urn:q> owl:sameAs _:right . <urn:x> <urn:p> <urn:y> ; <urn:q> <urn:y> .",
    "negative": "<urn:p> rdfs:subPropertyOf _:predicate . <urn:x> <urn:p> <urn:y> . <urn:assertion> owl:sourceIndividual <urn:x> ; owl:assertionProperty _:predicate ; owl:targetIndividual <urn:y> .",
    "max-zero": "<urn:p> rdfs:subPropertyOf _:predicate . <urn:x> a <urn:R> ; <urn:p> <urn:y> . <urn:R> owl:onProperty _:predicate ; owl:maxCardinality 0 .",
    "all-disjoint": "<urn:p> rdfs:subPropertyOf _:left, _:right . <urn:x> <urn:p> <urn:y> . <urn:set> a owl:AllDisjointProperties ; owl:members (_:left _:right <urn:unused>) .",
}
LIST = """
<urn:set> a owl:AllDisjointClasses ; owl:members HEAD .
<urn:A> <urn:firstBack> HEAD . <urn:firstBack> owl:inverseOf rdf:first .
<urn:tail> <urn:restBack> HEAD . <urn:restBack> owl:inverseOf rdf:rest .
<urn:tail> rdf:first <urn:B> ; rdf:rest rdf:nil .
<urn:x> a <urn:A>, <urn:B> .
"""


def messages(graph):
    return {str(value) for value in graph.objects(None, ERR.error)}


@pytest.fixture(params=["dataset", "store", "transaction", "persistent"])
def api(request):
    return request.param


def compare(document, api, tmp_path):
    source = Graph().parse(data=PREFIX + document, format="turtle")
    expected = Graph()
    for triple in source:
        expected.add(triple)
    DeductiveClosure(OWLRL_Semantics).expand(expected)
    ordinary = Graph()
    for triple in expected:
        if not isinstance(triple[0], Literal) and isinstance(triple[1], URIRef):
            ordinary.add(triple)

    def check(native):
        native.infer(inference="owlrl")
        actual = Graph().parse(data=native.serialize(format="ntriples"), format="nt")
        assert messages(actual) == messages(expected)
        assert isomorphic(actual, ordinary)

    def graph(owner):
        native = NativeGraph(owner, URIRef("urn:source"), bind_namespaces="none")
        for triple in source:
            native.add(triple)
        return native

    if api == "dataset":
        check(Dataset.from_rdflib(source))
    else:
        path = tmp_path / "store" if api == "persistent" else None
        with Store(path) as store:
            if api == "transaction":
                with store.transaction() as transaction:
                    check(graph(transaction))
            else:
                check(graph(store))
        if api == "persistent":
            with Store(path) as reopened:
                native = NativeGraph(reopened, URIRef("urn:source"), bind_namespaces="none")
                actual = Graph().parse(data=native.serialize(format="ntriples"), format="nt")
                assert isomorphic(actual, ordinary)
    return expected


@pytest.mark.parametrize("case", PREDICATES)
def test_non_iri_predicates_match_reference(case, api, tmp_path):
    compare(PREDICATES[case], api, tmp_path)


@pytest.mark.parametrize(
    "head,truthy",
    [
        ('"cell"', True),
        ('""', False),
        ('"0"^^xsd:integer', False),
        ('"1"^^xsd:integer', True),
        ('"false"^^xsd:boolean', False),
        ('"true"^^xsd:boolean', True),
        ('"0.0"^^xsd:decimal', False),
        ('"NaN"^^xsd:decimal', True),
        ('"PT0S"^^xsd:duration', False),
        ('"PT1S"^^xsd:duration', True),
        ('"!"^^xsd:base64Binary', False),
        ('"YQ=="^^xsd:base64Binary', True),
    ],
)
@pytest.mark.parametrize("position", ["head", "rest"])
def test_literal_list_head_uses_value_truthiness(head, truthy, position, api, tmp_path):
    document = LIST
    if position == "rest":
        document = document.replace("owl:members HEAD", "owl:members <urn:first>")
        document = document.replace("<urn:A> <urn:firstBack> HEAD", "<urn:B> <urn:firstBack> HEAD")
        document = document.replace("<urn:tail> <urn:restBack> HEAD", "rdf:nil <urn:restBack> HEAD")
        document += "<urn:first> rdf:first <urn:A> ; rdf:rest HEAD ."
    expected = compare(document.replace("HEAD", head), api, tmp_path)
    disjoint = {value for value in messages(expected) if value.startswith("Disjoint classes")}
    assert bool(disjoint) is truthy


@pytest.mark.parametrize("case", BLANKS)
def test_blank_predicates_match_reference(case, api, tmp_path):
    expected = compare(BLANKS[case], api, tmp_path)
    assert messages(expected)


@pytest.mark.parametrize(
    "head,truthy", [('"cell"', True), ('"1"^^xsd:integer', True), ('"0"^^xsd:integer', False)]
)
def test_literal_list_cycles_fail_atomically(head, truthy):
    document = PREFIX + LIST.replace("rdf:rest rdf:nil", "rdf:rest HEAD").replace("HEAD", head)
    source = Graph().parse(data=document, format="turtle")
    native = Dataset.from_rdflib(source)
    before = native.serialize(format="ntriples")
    if truthy:
        with pytest.raises(ValueError, match="recursive rdf:rest"):
            DeductiveClosure(OWLRL_Semantics).expand(source)
        with pytest.raises(ValueError, match="recursive rdf:rest"):
            native.infer(inference="owlrl")
        assert native.serialize(format="ntriples") == before
        assert native._last_inference is None
    else:
        DeductiveClosure(OWLRL_Semantics).expand(source)
        native.infer(inference="owlrl")
