"""Graph reads validate every term and preserve generalized RDF roles."""

import json

import pytest
from rdflib import BNode, Literal, URIRef
from rdflib import Graph as ReferenceGraph
from rdflib.compare import graph_diff, isomorphic, similar, to_isomorphic
from rdflib_turbo import Graph

SUBJECT = URIRef("urn:s")
PREDICATE = URIRef("urn:p")
OBJECT = Literal("value")


@pytest.mark.parametrize(
    "subject,predicate",
    [(Literal("subject"), None), (None, Literal("predicate")), (None, BNode("predicate"))],
)
@pytest.mark.parametrize("identifier", [URIRef("urn:g"), BNode("g")])
@pytest.mark.parametrize("transactional", [False, True])
def test_impossible_standard_rdf_positions_match_reference(
    subject, predicate, identifier, transactional
):
    expected = ReferenceGraph(identifier=identifier)
    expected.add((SUBJECT, PREDICATE, OBJECT))
    actual = Graph(identifier=identifier)
    actual.add((SUBJECT, PREDICATE, OBJECT))

    def compare(graph):
        pattern = (subject, predicate, None)
        assert list(graph.triples(pattern)) == list(expected.triples(pattern)) == []
        assert list(graph.objects(subject, predicate)) == []
        assert list(graph.predicates(subject, None)) == list(expected.predicates(subject, None))
        exact = (subject or SUBJECT, predicate or PREDICATE, OBJECT)
        assert (exact in graph) == (exact in expected) is False

    if transactional:
        with actual.transaction() as pending:
            compare(pending)
    else:
        compare(actual)


def symmetric_graph(cls, prefix):
    graph = cls()
    for name in ("a", "b"):
        graph.add((BNode(prefix + name), PREDICATE, OBJECT))
    return graph


def test_reference_graph_comparison_algorithms_accept_symmetric_blanks():
    actual = symmetric_graph(Graph, "actual")
    expected = symmetric_graph(ReferenceGraph, "reference")
    assert isomorphic(actual, expected)
    assert similar(actual, expected)
    common, only_actual, only_expected = graph_diff(to_isomorphic(actual), to_isomorphic(expected))
    assert len(common) == 2 and not only_actual and not only_expected
    assert to_isomorphic(actual).graph_digest() == to_isomorphic(expected).graph_digest()


def test_later_literal_metadata_errors_are_not_swallowed_by_impossible_subject():
    class BrokenLiteral(Literal):
        @property
        def datatype(self):
            raise RuntimeError("late literal metadata")

    graph = Graph()
    bad = BrokenLiteral("value")
    with pytest.raises(RuntimeError, match="late literal metadata"):
        list(graph.triples((Literal("subject"), None, bad)))
    with pytest.raises(RuntimeError, match="late literal metadata"):
        (Literal("subject"), PREDICATE, bad) in graph


@pytest.mark.parametrize(
    "subject,predicate",
    [
        (Literal("subject"), PREDICATE),
        (SUBJECT, BNode("predicate")),
        (SUBJECT, Literal("predicate")),
    ],
)
def test_mutations_preserve_generalized_roles_like_reference(subject, predicate):
    graph = Graph()
    reference = ReferenceGraph()
    statement = (subject, predicate, OBJECT)
    graph.add(statement)
    reference.add(statement)
    assert set(graph) == set(reference)
    assert statement in graph
    graph.remove(statement)
    reference.remove(statement)
    assert set(graph) == set(reference) == set()


def test_invalid_mutation_metadata_leaves_graph_unchanged():
    graph = Graph()
    statement = (SUBJECT, PREDICATE, OBJECT)
    graph.add(statement)
    with pytest.raises(ValueError, match="datatype and language"):
        graph.store.backend._native.insert_terms(
            ("uri", "urn:discard", None, None),
            ("uri", str(PREDICATE), None, None),
            ("literal", "invalid", "urn:type", "en"),
            None,
        )
    assert set(graph) == {statement}


@pytest.mark.parametrize("membership", [False, True])
def test_impossible_pattern_still_observes_closed_owner(membership):
    graph = Graph()
    graph.store.backend.close()
    with pytest.raises(RuntimeError, match="closed"):
        if membership:
            (Literal("subject"), PREDICATE, OBJECT) in graph
        else:
            list(graph.triples((Literal("subject"), None, None)))


def test_impossible_pattern_on_expired_transaction_fails():
    graph = Graph()
    with graph.transaction() as pending:
        pending.add((SUBJECT, PREDICATE, OBJECT))
    with pytest.raises(RuntimeError, match="closed"):
        list(pending.triples((Literal("subject"), None, None)))


def test_later_raw_metadata_is_validated_even_if_subject_cannot_match():
    from rdflib_turbo._literal import restore
    from rdflib_turbo._native import NativeTermFactories

    native = Graph().store.backend._native
    factories = NativeTermFactories(URIRef, BNode, restore, str.__new__, True)
    with pytest.raises(ValueError, match="datatype and language"):
        native.read_pattern(
            (("literal", "subject", None, None), None, ("literal", "object", "urn:datatype", "en")),
            None,
            7,
            factories,
        )


@pytest.mark.parametrize("membership", [False, True])
def test_later_metadata_reentry_can_close_impossible_pattern_owner(membership):
    graph = Graph()
    events = []

    class Closing(Literal):
        @property
        def datatype(self):
            events.append("datatype")
            graph.store.backend.close()
            return None

        @property
        def language(self):
            events.append("language")
            return None

    pattern = (Literal("subject"), PREDICATE, Closing("object"))
    with pytest.raises(RuntimeError, match="closed"):
        if membership:
            pattern in graph
        else:
            list(graph.triples(pattern))
    assert events == ["datatype", "language"]


@pytest.mark.parametrize(
    "subject,predicate", [(Literal("subject"), None), (None, BNode("predicate"))]
)
def test_impossible_json_reads_do_not_abort_pending_writes(subject, predicate):
    from rdflib_turbo._operations import _encode

    graph = Graph()
    source = json.dumps([_encode(subject), _encode(predicate), None, None])
    with graph.transaction() as pending:
        pending.add((SUBJECT, PREDICATE, OBJECT))
        native = pending.store.backend._native
        assert json.loads(native.match_quads(source)) == []
        assert native.count_pattern(source) == 0
    assert set(graph) == {(SUBJECT, PREDICATE, OBJECT)}
