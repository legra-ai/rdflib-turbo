"""Graph UPDATE defaults refer to that exact native graph, including blank names."""

import pytest
from rdflib import BNode, Literal, Namespace
from rdflib import Dataset as ReferenceDataset
from rdflib.namespace import XSD
from rdflib_turbo import Graph, Store, StoreDataset

EX = Namespace("urn:scoped-update:")
UPDATES = [
    "INSERT DATA {<urn:new> <urn:scoped-update:p> <urn:o>}",
    "DELETE DATA {<urn:scoped-update:a> <urn:scoped-update:p> 'A'}",
    "INSERT {?s <urn:scoped-update:q> ?v} WHERE {?s <urn:scoped-update:p> ?v}",
    "DELETE {?s <urn:scoped-update:p> ?v} INSERT {?s <urn:scoped-update:q> ?v} WHERE {?s <urn:scoped-update:p> ?v}",
    "DELETE WHERE {?s <urn:scoped-update:p> ?v}",
    "CLEAR DEFAULT",
    "INSERT {?s <urn:scoped-update:q> ?v} WHERE {}; INSERT {?s <urn:scoped-update:r> ?v} WHERE {}",
]


def populate(dataset, identifier):
    selected = dataset.default_graph if identifier is None else dataset.graph(identifier)
    selected.add((EX.selected, EX.p, Literal("scope")))
    selected.add((EX.a, EX.p, Literal("A")))
    dataset.graph(EX.other).add((EX.other, EX.p, Literal("other")))
    if identifier is not None:
        dataset.default_graph.add((EX.default, EX.p, Literal("default")))
    return selected


@pytest.mark.parametrize("identifier", [None, EX.selected, BNode("graph-scope")])
@pytest.mark.parametrize("source", UPDATES)
def test_graph_update_matches_reference_and_retains_other_contexts(identifier, source):
    expected, actual = ReferenceDataset(), StoreDataset()
    reference, candidate = populate(expected, identifier), populate(actual, identifier)
    bindings = {"s": EX.bound, "v": Literal("01", datatype=XSD.integer, normalize=False)}
    assert reference.update(source, initBindings=bindings if "WHERE {}" in source else {}) is None
    assert candidate.update(source, initBindings=bindings if "WHERE {}" in source else {}) is None
    assert set(candidate) == set(reference)
    assert set(actual.quads()) == set(expected.quads())


@pytest.mark.parametrize("identifier", [EX.selected, BNode("graph-scope")])
def test_scoped_update_reads_pending_writes_and_shares_rollback(identifier, monkeypatch):
    def forbidden(*args, **kwargs):
        raise AssertionError("Reference SPARQL evaluation or input serialization was invoked")

    monkeypatch.setattr("rdflib.plugins.sparql.processor.evalUpdate", forbidden)
    monkeypatch.setattr("rdflib.Graph.serialize", forbidden)
    with Store() as store:
        with pytest.raises(ValueError, match="abort"), store.transaction() as transaction:
            graph = Graph(store=transaction, identifier=identifier)
            graph.add((EX.a, EX.p, Literal("pending")))
            graph.update("INSERT {?s <urn:scoped-update:q> ?o} WHERE {?s <urn:scoped-update:p> ?o}")
            assert (EX.a, EX.q, Literal("pending")) in graph
            assert len(StoreDataset(store=store)) == 0
            raise ValueError("abort")
        assert len(store) == 0
        with store.transaction() as transaction:
            graph = Graph(store=transaction, identifier=identifier)
            graph.add((EX.a, EX.p, EX.o))
            graph.update("DELETE WHERE {?s ?p ?o}")
            assert len(graph) == 0
        assert len(store) == 0


@pytest.mark.parametrize("identifier", [EX.selected, BNode("graph-scope")])
def test_caught_scoped_update_syntax_error_dooms_transaction(identifier):
    with Store() as store:
        with pytest.raises(RuntimeError, match="closed"), store.transaction() as transaction:
            graph = Graph(store=transaction, identifier=identifier)
            graph.add((EX.pending, EX.p, EX.o))
            with pytest.raises(ValueError):
                graph.update("INVALID UPDATE")
        assert len(store) == 0


def test_caught_native_invalid_scope_aborts_prior_mutations():
    with Store() as store:
        with pytest.raises(RuntimeError, match="closed"), store.transaction() as transaction:
            graph = Graph(store=transaction, identifier=EX.selected)
            graph.add((EX.pending, EX.p, EX.o))
            with pytest.raises(ValueError):
                transaction._native.update("INSERT DATA {}", "{}", '{"type":"unknown"}')
        assert len(store) == 0


def test_invalid_update_binding_keeps_committed_store_open_and_unchanged():
    graph = StoreDataset().graph(EX.selected)
    graph.add((EX.existing, EX.p, EX.o))
    with pytest.raises(TypeError, match="Unsupported RDF term"):
        graph.update("INSERT {?s <urn:scoped-update:q> ?v} WHERE {}", initBindings={"v": object()})
    assert set(graph) == {(EX.existing, EX.p, EX.o)}
    graph.update("INSERT DATA {<urn:new> <urn:scoped-update:p> <urn:o>}")
    assert len(graph) == 2
