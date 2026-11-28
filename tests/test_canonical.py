"""RDFC-1.0 canonical N-Quads over native Graph, Dataset and transaction scopes."""

import time

import pytest
import rdflib
from rdflib import BNode, Literal, URIRef
from rdflib.namespace import XSD
from rdflib_turbo import CanonicalizationBudgetExceeded, Dataset, Graph, HashAlgorithm

P = URIRef("http://example.com/p")
DIAMOND = """
_:a <http://example.com/p> _:b .
_:a <http://example.com/p> _:c .
_:b <http://example.com/p> _:d .
_:c <http://example.com/p> _:d .
"""


def clique(size):
    nodes = [BNode() for _ in range(size)]
    graph = Graph()
    for subject in nodes:
        for obj in nodes:
            graph.add((subject, P, obj))
    return graph


def test_graph_canonical_form_relabels_blank_nodes_deterministically():
    one = Graph().parse(data=DIAMOND, format="nt")
    other = Graph().parse(data=DIAMOND.replace("_:a", "_:z"), format="nt")
    text = one.canonicalize()
    assert text == other.canonicalize()
    labels = {word for line in text.splitlines() for word in line.split() if word[:2] == "_:"}
    assert labels == {"_:c14n0", "_:c14n1", "_:c14n2", "_:c14n3"}
    assert all(line.endswith(" .") for line in text.splitlines())


def test_hash_algorithm_is_selectable_and_validated():
    graph = Graph().parse(data=DIAMOND, format="nt")
    sha256 = graph.canonicalize()
    assert graph.canonicalize(hash_algorithm=HashAlgorithm.SHA256) == sha256
    assert graph.canonicalize(hash_algorithm="sha384").count("\n") == 4
    with pytest.raises(ValueError, match="md5"):
        graph.canonicalize(hash_algorithm="md5")


def test_canonical_form_uses_stored_lexical_forms():
    graph = Graph().parse(
        format="turtle",
        data="""
        @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
        [] <http://example.com/p> "01"^^xsd:integer, "1.50"^^xsd:decimal, "1"^^xsd:boolean,
            "+1E0"^^xsd:double, "chat"@FR, "a\\tb" .
        """,
    )
    text = graph.canonicalize()
    for term in (
        '"01"^^<http://www.w3.org/2001/XMLSchema#integer>',
        '"1.50"^^<http://www.w3.org/2001/XMLSchema#decimal>',
        '"1"^^<http://www.w3.org/2001/XMLSchema#boolean>',
        '"+1E0"^^<http://www.w3.org/2001/XMLSchema#double>',
        '"chat"@fr',
        '"a\\tb"',
    ):
        assert f"_:c14n0 <http://example.com/p> {term} .\n" in text


def test_unnormalized_python_literals_keep_their_lexical_form(monkeypatch):
    monkeypatch.setattr(rdflib, "NORMALIZE_LITERALS", False)
    graph = Graph()
    graph.add((URIRef("urn:s"), P, Literal("007", datatype=XSD.integer)))
    assert graph.canonicalize() == (
        '<urn:s> <http://example.com/p> "007"^^<http://www.w3.org/2001/XMLSchema#integer> .\n'
    )
    assert not graph.isomorphic(
        Graph().parse(data=graph.canonicalize().replace("007", "7"), format="nt")
    )


def test_explicit_string_is_rdf11_in_output_but_distinct_for_isomorphism():
    plain = Graph()
    plain.add((URIRef("urn:s"), P, Literal("a")))
    typed = Graph()
    typed.add((URIRef("urn:s"), P, Literal("a", datatype=XSD.string)))
    assert plain.canonicalize() == typed.canonicalize() == '<urn:s> <http://example.com/p> "a" .\n'
    assert not plain.isomorphic(typed)
    assert plain.isomorphic(typed) == rdflib.compare.isomorphic(
        rdflib.Graph() + plain, rdflib.Graph() + typed
    )


def test_poison_graph_exhausts_the_budget_quickly():
    graph = clique(10)
    started = time.monotonic()
    with pytest.raises(CanonicalizationBudgetExceeded, match="work budget of 1000000"):
        graph.canonicalize()
    assert time.monotonic() - started < 10
    assert issubclass(CanonicalizationBudgetExceeded, RuntimeError)
    with pytest.raises(CanonicalizationBudgetExceeded, match="work budget of 5 units"):
        clique(3).canonicalize(work_budget=5)
    assert clique(3).canonicalize(work_budget=1_000).count("\n") == 9


@pytest.mark.parametrize(
    "budget, error", [(0, ValueError), (-1, ValueError), (True, TypeError), (1.0, TypeError)]
)
def test_work_budget_is_a_positive_int(budget, error):
    with pytest.raises(error, match="work_budget"):
        Graph().canonicalize(work_budget=budget)


def test_dataset_canonical_form_keeps_graph_names():
    dataset = Dataset()
    dataset.graph(URIRef("urn:g")).add((BNode(), P, Literal("x")))
    dataset.add((URIRef("urn:s"), P, URIRef("urn:o")))
    assert dataset.canonicalize() == (
        '<urn:s> <http://example.com/p> <urn:o> .\n_:c14n0 <http://example.com/p> "x" <urn:g> .\n'
    )
    assert dataset.graph(URIRef("urn:g")).canonicalize() == (
        '_:c14n0 <http://example.com/p> "x" .\n'
    )


def test_transaction_sees_pending_writes_and_survives_an_exhausted_budget():
    graph = Graph()
    with graph.transaction() as pending:
        for triple in clique(10):
            pending.add(triple)
        pending.add((URIRef("urn:s"), P, Literal("pending")))
        with pytest.raises(CanonicalizationBudgetExceeded):
            pending.canonicalize()
        pending.remove((None, P, None))
        pending.add((URIRef("urn:s"), P, Literal("pending")))
        assert pending.canonicalize() == '<urn:s> <http://example.com/p> "pending" .\n'
        assert graph.canonicalize() == ""
    assert graph.canonicalize() == '<urn:s> <http://example.com/p> "pending" .\n'


def test_invalid_transaction_request_finishes_the_transaction_like_dump():
    graph = Graph()
    with pytest.raises(ValueError, match="md5"):
        with graph.transaction() as pending:
            pending.add((URIRef("urn:s"), P, Literal("x")))
            pending.canonicalize(hash_algorithm="md5")
    assert len(graph) == 0
