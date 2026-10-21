"""Diagnostic RDF and destination namespaces share the graph transaction."""

import pytest
from owlrl import OWLRL_Semantics
from rdflib import BNode, Namespace
from rdflib import Dataset as ReferenceDataset
from rdflib import Graph as ReferenceGraph
from rdflib.compare import isomorphic
from rdflib.namespace import OWL, RDF
from rdflib_turbo import Graph, Store, StoreDataset

EX = Namespace("urn:diagnostic-namespaces:")
ERR = Namespace("http://www.daml.org/2002/03/agents/agent-ont#")
KINDS = ["default", "named", "blank", "dataset"]
BINDINGS = [
    [],
    [("err", EX.occupied)],
    [("fault", ERR)],
    [("err", EX.occupied), ("err1", EX.other)],
    [("err", ERR)],
    [("fault", ERR), ("err", EX.occupied)],
]


def owners(kind, native, separate):
    if kind == "dataset":
        source = StoreDataset() if native else ReferenceDataset()
        input_graph = source.graph(EX.source)
        destination = source.default_graph
    else:
        identifier = BNode("scope") if kind == "blank" else EX.source
        if native:
            source = (
                Graph(bind_namespaces="none")
                if kind == "default"
                else Graph(store=Store(), identifier=identifier, bind_namespaces="none")
            )
        else:
            source = ReferenceGraph(identifier=identifier, bind_namespaces="none")
        input_graph = destination = source
    if separate:
        graph_type = Graph if native else ReferenceGraph
        destination = graph_type(
            store=source.store, identifier=EX.destination, bind_namespaces="none"
        )
    return source, input_graph, destination


@pytest.mark.parametrize("kind", KINDS)
@pytest.mark.parametrize("separate", [False, True])
@pytest.mark.parametrize("bindings", BINDINGS)
def test_diagnostic_namespace_and_destination_match_original(kind, separate, bindings):
    reference, reference_input, reference_destination = owners(kind, False, separate)
    native, native_input, native_destination = owners(kind, True, separate)
    for input_graph, destination in [
        (reference_input, reference_destination),
        (native_input, native_destination),
    ]:
        input_graph.add((EX.conflict, RDF.type, OWL.Nothing))
        for prefix, namespace in bindings:
            destination.bind(prefix, namespace)
    OWLRL_Semantics(
        reference, False, False, destination=reference_destination if separate else None
    ).closure()
    native.infer(destination=native_destination if separate else None)
    assert dict(native_destination.namespaces()) == dict(reference_destination.namespaces())
    assert isomorphic(native_destination, reference_destination)
    assert len(list(native_destination.subjects(RDF.type, ERR.ErrorMessage))) == 1
    assert native.store.backend._last_inference.error_messages


@pytest.mark.parametrize("kind", KINDS)
def test_consistent_inference_does_not_bind_diagnostic_namespace(kind):
    graph, input_graph, _ = owners(kind, True, False)
    input_graph.add((EX.x, RDF.type, EX.Class))
    graph.bind("err", EX.occupied)
    before = dict(graph.namespaces())
    graph.infer()
    assert dict(graph.namespaces()) == before
    assert graph.store.backend._last_inference.error_messages == []


@pytest.mark.parametrize("kind", KINDS)
@pytest.mark.parametrize("abort", [False, True])
def test_explicit_namespaces_are_provisional_and_publish_only_after_commit(kind, abort):
    graph, input_graph, _ = owners(kind, True, False)
    input_graph.add((EX.conflict, RDF.type, OWL.Nothing))
    before = dict(graph.namespaces())
    failure = RuntimeError("caller abort")
    provisional = None
    try:
        with graph.transaction() as transaction:
            transaction.infer()
            provisional = transaction.store.backend._last_inference
            assert any(str(namespace) == str(ERR) for _, namespace in transaction.namespaces())
            assert dict(graph.namespaces()) == before
            assert graph.store.backend._last_inference is None
            if abort:
                raise failure
    except RuntimeError as caught:
        assert abort and caught is failure
    assert provisional.error_messages
    if abort:
        assert dict(graph.namespaces()) == before
        assert len(input_graph) == 1
        assert graph.store.backend._last_inference is None
    else:
        assert any(str(namespace) == str(ERR) for _, namespace in graph.namespaces())


@pytest.mark.parametrize("kind", KINDS)
@pytest.mark.parametrize("explicit", [False, True])
@pytest.mark.parametrize("separate", [False, True])
def test_binding_failure_aborts_rdf_and_metadata_even_when_caught(
    kind, explicit, separate, monkeypatch
):
    graph, input_graph, destination = owners(kind, True, separate)
    input_graph.add((EX.conflict, RDF.type, OWL.Nothing))
    before = dict(graph.namespaces())
    failure = ValueError("diagnostic namespace binding failed")
    bind = ReferenceGraph.bind

    def reject(owner, prefix, namespace, *args, **kwargs):
        if str(namespace) == str(ERR):
            assert owner.identifier == destination.identifier
            # Callback sees the committed owner, not prematurely published output.
            assert len(input_graph) == 1
            assert graph.store.backend._last_inference is None
            raise failure
        return bind(owner, prefix, namespace, *args, **kwargs)

    monkeypatch.setattr(ReferenceGraph, "bind", reject)
    if explicit:
        with pytest.raises(RuntimeError, match="closed"):
            with graph.transaction() as transaction:
                with pytest.raises(ValueError) as caught:
                    transaction.infer(destination=destination.identifier if separate else None)
                assert caught.value is failure
                assert transaction.store.backend._last_inference is None
                assert dict(transaction.namespaces()) == before
                # A caught Python publication failure still dooms native writes.
    else:
        with pytest.raises(ValueError) as caught:
            graph.infer(destination=destination if separate else None)
        assert caught.value is failure
    assert len(input_graph) == 1
    assert dict(graph.namespaces()) == before
    assert graph.store.backend._last_inference is None


@pytest.mark.parametrize("method", ["graph", "get_context"])
def test_dataset_contexts_retain_the_owner_namespace_manager(method):
    for dataset in [ReferenceDataset(), StoreDataset()]:
        context = getattr(dataset, method)(EX.context)
        assert context.namespace_manager is dataset.namespace_manager
