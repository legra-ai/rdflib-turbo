"""Reading accepted identifiers restores terms without validating the input again."""

import logging

import pytest
from rdflib import BNode, Literal, URIRef
from rdflib_turbo import Graph


@pytest.mark.parametrize("transactional", [False, True])
def test_iteration_does_not_repeat_input_iri_warnings(caplog, transactional):
    identifier = URIRef("urn:accepted with space")
    triple = (identifier, URIRef("urn:p"), BNode("caller-blank"))
    graph = Graph().add(triple)
    caplog.clear()
    with caplog.at_level(logging.WARNING, logger="rdflib.term"):
        if transactional:
            with graph.transaction() as view:
                actual = list(view)
        else:
            actual = list(graph)
    assert actual == [triple]
    assert type(actual[0][0]) is URIRef
    assert type(actual[0][2]) is BNode
    assert caplog.records == []


def test_graph_triples_does_not_construct_unused_store_context_results(monkeypatch):
    triple = (URIRef("urn:s"), URIRef("urn:p"), BNode("object"))
    graph = Graph().add(triple)

    def forbidden(*args, **kwargs):
        raise AssertionError("Graph.triples requested unused Store context results")

    monkeypatch.setattr(graph.store, "triples", forbidden)
    assert list(graph) == [triple]
    assert list(graph.triples((triple[0], None, None))) == [triple]


@pytest.mark.parametrize(
    "lexical",
    ["", " leading and trailing ", "é 🙂\nline", "x" * 4096],
    ids=["empty", "whitespace", "unicode", "long"],
)
@pytest.mark.parametrize("language", [None, "fr"])
def test_plain_literal_restoration_preserves_public_state_without_recasting(
    monkeypatch, lexical, language
):
    import pickle

    import rdflib.term

    expected = Literal(lexical, lang=language, normalize=False)
    graph = Graph().add((URIRef("urn:s"), URIRef("urn:p"), expected))

    def forbidden(*args, **kwargs):
        raise AssertionError("Stored plain literal was cast again")

    with monkeypatch.context() as patch:
        patch.setattr(rdflib.term, "_castLexicalToPython", forbidden)
        actual = next(graph.objects())
    assert type(actual) is Literal
    assert str(actual) == lexical
    assert actual == expected
    assert hash(actual) == hash(expected)
    assert actual.language == language
    assert actual.datatype is None
    assert actual.value == lexical
    assert actual.toPython() == lexical
    assert actual.ill_typed is None
    assert actual.n3() == expected.n3()
    assert pickle.loads(pickle.dumps(actual)) == expected
