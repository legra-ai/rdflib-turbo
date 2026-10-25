"""Programmatic generalized OWL facts retain original observable graph state.

All fixtures are public synthetic data. RDF syntax parsers remain strict; these
cases deliberately enter through RDFLib's programmatic term API.
"""

import pytest
from rdflib import BNode, Dataset, Graph, Literal, URIRef
from rdflib.namespace import RDF, RDFS, XSD

CASES = (
    "ordinary",
    "literal_object_typing",
    "literal_subject",
    "blank_predicate_cleanup",
    "literal_predicate_retained",
    "new_literal_predicate_refused",
)


def facts(case):
    s, p, o, parent = map(URIRef, ("urn:s", "urn:p", "urn:o", "urn:parent"))
    if case == "ordinary":
        return [(s, p, o), (p, RDFS.domain, parent)]
    if case == "literal_object_typing":
        return [(s, p, Literal("7", datatype=XSD.integer, normalize=False))]
    if case == "literal_subject":
        return [(Literal("subject", lang="en"), p, o), (p, RDFS.domain, parent)]
    if case == "blank_predicate_cleanup":
        predicate = BNode("generalized-property")
        return [(s, predicate, o), (predicate, RDFS.subPropertyOf, parent)]
    if case == "literal_predicate_retained":
        return [(s, Literal("predicate"), o)]
    if case == "new_literal_predicate_refused":
        return [(s, p, o), (p, RDFS.subPropertyOf, Literal("forbidden"))]
    raise AssertionError(case)


def original(case, *, separate=False):
    from owlrl import DeductiveClosure, OWLRL_Semantics

    dataset = Dataset()
    source = dataset.graph(URIRef("urn:source-a"))
    destination = dataset.graph(BNode("destination-b")) if separate else source
    for triple in facts(case):
        source.add(triple)
    if separate and case == "blank_predicate_cleanup":
        destination.add(facts(case)[0])
        destination.add((URIRef("urn:existing"), BNode("generalized-property"), URIRef("urn:v")))
    DeductiveClosure(OWLRL_Semantics, axiomatic_triples=False, datatype_axioms=False).expand(
        source, destination=destination
    )
    return source, destination


@pytest.mark.parametrize("case", CASES)
def test_generalized_owl_functional_result_matches_original_without_mutating_caller(case):
    from owlrl_turbo import expand

    source = Graph()
    for triple in facts(case):
        source.add(triple)
    before = set(source)
    expected, _ = original(case)
    actual = expand(source, inference="owlrl", inplace=False)
    assert set(source) == before
    assert set(actual) == set(expected)


@pytest.mark.parametrize("case", CASES)
def test_generalized_owl_inplace_preserves_exact_original_additions_and_removals(case):
    from owlrl_turbo import expand

    source = Graph()
    for triple in facts(case):
        source.add(triple)
    expected, _ = original(case)
    returned = expand(source, inference="owlrl", inplace=True)
    assert returned is source
    assert set(source) == set(expected)


@pytest.mark.parametrize("case", CASES)
def test_native_generalized_owl_selected_source_and_destination_match_original(case):
    from rdflib_turbo import Graph as NativeGraph
    from rdflib_turbo import Store

    store = Store()
    source = NativeGraph(store=store, identifier=URIRef("urn:source-a"))
    destination = NativeGraph(store=store, identifier=BNode("destination-b"))
    outside = NativeGraph(store=store, identifier=URIRef("urn:unrelated"))
    outside.add((URIRef("urn:outside"), URIRef("urn:p"), URIRef("urn:v")))
    before_outside = set(outside)
    for triple in facts(case):
        source.add(triple)
    if case == "blank_predicate_cleanup":
        destination.add(facts(case)[0])
        destination.add((URIRef("urn:existing"), BNode("generalized-property"), URIRef("urn:v")))
    expected_source, expected_destination = original(case, separate=True)
    assert source.infer(inference="owlrl", destination=destination) is source
    assert set(source) == set(expected_source)
    assert set(destination) == set(expected_destination)
    assert set(outside) == before_outside


def test_original_generalized_terms_and_exact_cleanup_contracts():
    source, _ = original("literal_object_typing")
    value = Literal("7", datatype=XSD.integer, normalize=False)
    assert (value, RDF.type, XSD.integer) in source
    source, _ = original("literal_subject")
    assert (Literal("subject", lang="en"), RDF.type, URIRef("urn:parent")) in source
    source, destination = original("blank_predicate_cleanup", separate=True)
    assert not any(isinstance(predicate, BNode) for _, predicate, _ in source)
    assert facts("blank_predicate_cleanup")[0] not in destination
    # Cleanup is source-enumerated exact SPO, not predicate-wide destination deletion.
    assert (URIRef("urn:existing"), BNode("generalized-property"), URIRef("urn:v")) in destination
    assert (URIRef("urn:s"), URIRef("urn:parent"), URIRef("urn:o")) in destination
    source, _ = original("literal_predicate_retained")
    assert facts("literal_predicate_retained")[0] in source
    source, _ = original("new_literal_predicate_refused")
    assert (URIRef("urn:s"), Literal("forbidden"), URIRef("urn:o")) not in source


@pytest.mark.parametrize("case", CASES)
def test_generalized_owl_delta_contains_only_new_facts_and_owner_has_exact_cleanup(case):
    from rdflib_turbo import Dataset as NativeDataset

    source = Graph()
    for triple in facts(case):
        source.add(triple)
    before = set(source)
    expected, _ = original(case)
    owner = NativeDataset.from_rdflib(source)
    delta = owner.infer_delta(inference="owlrl")
    assert set(delta.to_rdflib().default_graph) == set(expected) - before
    assert set(owner.to_rdflib().default_graph) == set(expected)
    assert set(source) == before


def test_original_source_cleanup_preserves_same_lexical_graph_kinds_and_other_contexts():
    from owlrl import DeductiveClosure, OWLRL_Semantics

    dataset = Dataset()
    source = dataset.graph(URIRef("urn:same-scope"))
    destination = dataset.graph(BNode("urn:same-scope"))
    other = dataset.graph(URIRef("urn:unrelated"))
    for triple in facts("blank_predicate_cleanup"):
        source.add(triple)
        destination.add(triple)
        other.add(triple)
    retained = (URIRef("urn:existing"), BNode("generalized-property"), URIRef("urn:v"))
    destination.add(retained)
    before_other = set(other)
    DeductiveClosure(OWLRL_Semantics, axiomatic_triples=False, datatype_axioms=False).expand(
        source, destination=destination
    )
    removed = facts("blank_predicate_cleanup")[0]
    assert removed not in source
    assert removed not in destination
    assert retained in destination
    assert set(other) == before_other
