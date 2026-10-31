"""Parsing a caller Graph subclass uses its actual insertion callbacks."""

import pytest
from rdflib import Graph as ReferenceGraph
from rdflib import URIRef
from rdflib_turbo import Graph

ROWS = [(URIRef("urn:" + subject), URIRef("urn:p"), URIRef("urn:o")) for subject in ("z", "a", "b")]
DOCUMENTS = [
    ("turtle", " ".join(f"<{s}> <{p}> <{o}>." for s, p, o in ROWS)),
    ("nt", "\n".join(f"<{s}> <{p}> <{o}> ." for s, p, o in ROWS) + "\n"),
]


@pytest.mark.parametrize("base", [ReferenceGraph, Graph], ids=["reference", "turbo"])
@pytest.mark.parametrize(("format", "document"), DOCUMENTS, ids=["turtle", "nt"])
def test_parse_calls_actual_subclass_add_in_document_order(base, format, document):
    events = []

    class Ordered(base):
        def add(self, triple):
            events.append((self, triple))
            return super().add(triple)

    graph = Ordered()
    assert graph.parse(data=document, format=format) is graph
    assert len(events) == len(ROWS)
    assert all(owner is graph for owner, _ in events)
    assert [triple for _, triple in events] == ROWS
    assert set(graph) == set(ROWS)


@pytest.mark.parametrize("base", [ReferenceGraph, Graph], ids=["reference", "turbo"])
@pytest.mark.parametrize(("format", "document"), DOCUMENTS, ids=["turtle", "nt"])
def test_parse_preserves_subclass_add_exception_identity_and_partial_input(base, format, document):
    failure = RuntimeError("caller insertion stopped")
    events = []

    class Interrupted(base):
        def add(self, triple):
            events.append((self, triple))
            if len(events) == 2:
                raise failure
            return super().add(triple)

    graph = Interrupted()
    with pytest.raises(RuntimeError) as caught:
        graph.parse(data=document, format=format)
    assert caught.value is failure
    assert all(owner is graph for owner, _ in events)
    assert [triple for _, triple in events] == ROWS[:2]
    assert set(graph) == {ROWS[0]}


@pytest.mark.parametrize(("format", "document"), DOCUMENTS, ids=["turtle", "nt"])
def test_caught_subclass_callback_error_aborts_explicit_native_transaction(format, document):
    failure = RuntimeError("caller insertion stopped")
    events = []

    class Interrupted(Graph):
        def add(self, triple):
            events.append((self, triple))
            if len(events) == 2:
                raise failure
            return super().add(triple)

    owner = Graph()
    with pytest.raises(RuntimeError, match="closed"):
        with owner.transaction() as pending:
            source = Interrupted(store=pending.store, identifier=pending.identifier)
            with pytest.raises(RuntimeError) as caught:
                source.parse(data=document, format=format)
            assert caught.value is failure
            assert all(graph is source for graph, _ in events)
    assert len(owner) == 0


@pytest.mark.parametrize(("format", "document"), DOCUMENTS, ids=["turtle", "nt"])
def test_closed_native_graph_parse_still_rolls_back_late_syntax_error(format, document):
    graph = Graph()
    kept = (URIRef("urn:kept"), URIRef("urn:p"), URIRef("urn:old"))
    graph.add(kept)
    with pytest.raises(Exception):
        graph.parse(data=document + "\nthis is not RDF", format=format)
    assert set(graph) == {kept}
