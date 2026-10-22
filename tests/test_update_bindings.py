"""Native UPDATE uses RDFLib's typed, per-operation initial context."""

import pytest
from rdflib import BNode, Literal, Namespace, Variable
from rdflib import Graph as ReferenceGraph
from rdflib.graph import DATASET_DEFAULT_GRAPH_ID
from rdflib.namespace import XSD
from rdflib_turbo import Graph, Store, StoreDataset

EX = Namespace("urn:")
UPDATES = [
    "INSERT {?s <urn:p> ?v} WHERE {}",
    "DELETE WHERE {?s <urn:p> ?v}",
    'INSERT {?s <urn:q> ?v} WHERE {BIND(<urn:b> AS ?s) BIND("new" AS ?v)}',
    "INSERT {?s <urn:p> ?v} WHERE {BIND(<urn:b> AS ?s)}; INSERT {?s <urn:q> ?v} WHERE {}",
    'INSERT DATA {<urn:c> <urn:q> "constant"}',
    "INSERT {?s <urn:q> ?v} WHERE {{SELECT ?v WHERE {?s <urn:p> ?v}}}",
    "INSERT {?s <urn:q> ?v} WHERE {VALUES ?s {<urn:b>}}",
]


@pytest.mark.parametrize("source", UPDATES)
@pytest.mark.parametrize("bound", [False, True])
def test_native_updates_match_reference_context(source, bound):
    bindings = (
        {"s": EX.a, "v": Literal("01", datatype=XSD.integer, normalize=False)} if bound else {}
    )
    if "DELETE WHERE" in source or "SELECT ?v" in source:
        bindings.pop("v", None)
    expected, actual = ReferenceGraph(), Graph()
    for graph in (expected, actual):
        graph.add((EX.a, EX.p, Literal("A")))
        graph.add((EX.b, EX.p, Literal("B")))
    assert expected.update(source, initBindings=bindings) is None
    assert actual.update(source, initBindings=bindings) is None
    assert set(actual) == set(expected)


@pytest.mark.parametrize("dataset", [False, True])
@pytest.mark.parametrize(
    "source, options, error",
    [
        ("INSERT {} WHERE {}", {"initBindings": {"s": object()}}, TypeError),
        (object(), {}, TypeError),
        ("INSERT {} WHERE {}", {"unexpected": True}, TypeError),
        ("INSERT {} WHERE {}", {"processor": "other"}, ValueError),
        ("INSERT {} WHERE {}", {"use_store_provided": False}, ValueError),
    ],
)
def test_caught_update_prevalidation_failure_dooms_transaction(dataset, source, options, error):
    with Store() as store:
        with pytest.raises(RuntimeError, match="closed"), store.transaction() as transaction:
            graph = (
                StoreDataset(store=transaction)
                if dataset
                else Graph(store=transaction, identifier=DATASET_DEFAULT_GRAPH_ID)
            )
            graph.add((EX.pending, EX.p, EX.o))
            with pytest.raises(error):
                graph.update(source, **options)
        assert len(store) == 0


def test_update_reads_pending_writes_and_statement_context_resets():
    with Store() as store:
        with store.transaction() as transaction:
            graph = StoreDataset(store=transaction)
            graph.add((EX.a, EX.p, Literal("pending")))
            graph.update(
                "INSERT {?s <urn:q> ?v} WHERE {?s <urn:p> ?v}; "
                "INSERT {?s <urn:r> ?v} WHERE {BIND(<urn:b> AS ?s)}; "
                "INSERT {?s <urn:t> ?v} WHERE {}",
                initBindings={"s": EX.a, "v": Literal("pending")},
            )
            assert (EX.a, EX.q, Literal("pending")) in graph
            assert (EX.b, EX.r, Literal("pending")) in graph
            assert (EX.a, EX.t, Literal("pending")) in graph
        assert len(StoreDataset(store=store)) == 4


@pytest.mark.parametrize("key", ["s", "?s", Variable("s")])
@pytest.mark.parametrize(
    "value", [BNode("binding"), Literal('" } UNION { ?s ?p ?o } #'), Literal("word", lang="en")]
)
def test_update_parameter_values_are_exact_terms_not_sparql_source(key, value, monkeypatch):
    def forbidden(*args, **kwargs):
        raise AssertionError("Python SPARQL update evaluator was invoked")

    monkeypatch.setattr("rdflib.plugins.sparql.processor.evalUpdate", forbidden)
    graph = Graph()
    graph.update("INSERT {?s <urn:p> ?v} WHERE {}", initBindings={key: EX.a, "v": value})
    assert set(graph) == {(EX.a, EX.p, value)}


def test_native_invalid_update_parameter_payload_aborts_transaction():
    with Store() as store:
        with pytest.raises(RuntimeError, match="closed"), store.transaction() as transaction:
            transaction.update("INSERT DATA {<urn:a> <urn:p> <urn:o>}")
            with pytest.raises(ValueError):
                transaction._native.update("INSERT {} WHERE {}", '{"s":{"type":"invalid"}}')
        assert len(store) == 0


def test_named_update_scope_binds_terms_and_commits_the_same_transaction():
    with Store() as store:
        with store.transaction() as transaction:
            graph = Graph(store=transaction, identifier=EX.named)
            graph.add((EX.a, EX.p, EX.o))
            graph.update("INSERT {?s <urn:q> ?v} WHERE {}", initBindings={"s": EX.a, "v": EX.o})
            assert (EX.a, EX.q, EX.o) in graph
        assert len(Graph(store=store, identifier=EX.named)) == 2
