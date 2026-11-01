"""Instance insertion callbacks retain the actual RDFLib parser sink."""

from types import MethodType

import pytest
from rdflib import Graph as ReferenceGraph
from rdflib_turbo import Graph

from .test_graph_subclass_parsing import DOCUMENTS, ROWS


def install_add(graph, callback, binding):
    if binding == "method":
        graph.add = MethodType(lambda owner, triple: callback(owner, triple), graph)
    else:
        graph.add = lambda triple: callback(graph, triple)


@pytest.mark.parametrize("base", [ReferenceGraph, Graph], ids=["reference", "turbo"])
@pytest.mark.parametrize("binding", ["function", "method"])
@pytest.mark.parametrize(("format", "document"), DOCUMENTS, ids=["turtle", "nt"])
def test_instance_add_is_called_on_actual_sink(base, binding, format, document):
    graph = base()
    insert = graph.add
    events = []

    def callback(owner, triple):
        events.append((owner, triple))
        return insert(triple)

    install_add(graph, callback, binding)
    assert graph.parse(data=document, format=format) is graph
    assert all(owner is graph for owner, _ in events)
    assert [triple for _, triple in events] == ROWS
    assert set(graph) == set(ROWS)


@pytest.mark.parametrize("base", [ReferenceGraph, Graph], ids=["reference", "turbo"])
@pytest.mark.parametrize("binding", ["function", "method"])
@pytest.mark.parametrize(("format", "document"), DOCUMENTS, ids=["turtle", "nt"])
def test_instance_callback_failure_preserves_identity_and_accepted_rows(
    base, binding, format, document
):
    graph = base()
    insert = graph.add
    failure = LookupError("instance insertion stopped")
    events = []

    def callback(owner, triple):
        events.append((owner, triple))
        if len(events) == 2:
            raise failure
        return insert(triple)

    install_add(graph, callback, binding)
    with pytest.raises(LookupError) as caught:
        graph.parse(data=document, format=format)
    assert caught.value is failure
    assert all(owner is graph for owner, _ in events)
    assert [triple for _, triple in events] == ROWS[:2]
    assert set(graph) == {ROWS[0]}


@pytest.mark.parametrize("binding", ["function", "method"])
@pytest.mark.parametrize(("format", "document"), DOCUMENTS, ids=["turtle", "nt"])
def test_caught_instance_callback_failure_aborts_native_transaction(binding, format, document):
    graph = Graph()
    failure = LookupError("instance insertion stopped")
    with pytest.raises(RuntimeError, match="closed"):
        with graph.transaction() as pending:
            insert = pending.add
            events = []

            def callback(owner, triple):
                events.append((owner, triple))
                if len(events) == 2:
                    raise failure
                return insert(triple)

            install_add(pending, callback, binding)
            with pytest.raises(LookupError) as caught:
                pending.parse(data=document, format=format)
            assert caught.value is failure
            assert all(owner is pending for owner, _ in events)
    assert len(graph) == 0
