"""Native graph inference preserves the caller's read and write scopes."""

import decimal

import pytest
from owlrl import DeductiveClosure, OWLRL_Semantics
from rdflib import BNode, Literal, Namespace
from rdflib import Graph as ReferenceGraph
from rdflib.namespace import OWL, RDF, RDFS, XSD
from rdflib_turbo import Graph, Store, StoreDataset

EX = Namespace("urn:scoped-inference:")


@pytest.mark.parametrize("union", [False, True])
@pytest.mark.parametrize("mode", ["rdfs", "owlrl", "both"])
def test_literal_identity_publication_uses_destination_membership(union, mode):
    source = StoreDataset() if union else Graph(store=Store(), identifier=EX.source)
    input_graph = source.graph(EX.source) if union else source
    statement = (EX.x, EX.label, Literal("label"))
    input_graph.add(statement)
    destination = (
        source.graph(EX.destination)
        if union
        else Graph(store=source.store, identifier=EX.destination)
    )
    source.infer(inference=mode, destination=destination)
    assert (statement in destination) is (mode == "rdfs")
    assert statement in input_graph
    # Repeated reference closure may add schema facts, but never duplicate this SPO.
    source.infer(inference=mode, destination=destination)
    assert len(list(destination.triples(statement))) == (1 if mode == "rdfs" else 0)
    assert len(input_graph) == 1


@pytest.mark.parametrize("identifier", [None, EX.source, BNode("source")])
def test_graph_inference_borrows_only_its_own_scope(identifier, monkeypatch):
    graph = Graph() if identifier is None else Graph(store=Store(), identifier=identifier)
    graph.add((EX.C, RDFS.subClassOf, EX.D))
    graph.add((EX.x, RDF.type, EX.C))
    outside = Graph(store=graph.store, identifier=EX.outside)
    outside.add((EX.D, RDFS.subClassOf, EX.Outside))

    def forbidden(*args, **kwargs):
        raise AssertionError("Inference must not serialize or iterate Python input triples")

    monkeypatch.setattr(ReferenceGraph, "serialize", forbidden)
    monkeypatch.setattr(Graph, "__iter__", forbidden)
    assert graph.infer(inference="rdfs") is graph
    assert (EX.x, RDF.type, EX.D) in graph
    assert (EX.x, RDF.type, EX.Outside) not in graph
    assert len(outside) == 1


def test_separate_destination_does_not_feed_inferred_facts_back_into_source():
    source = Graph(identifier=EX.source)
    source.add((EX.x, RDF.type, EX.C))
    source.add((EX.C, RDFS.subClassOf, EX.D))
    source.add((EX.D, RDFS.subClassOf, EX.E))
    destination = Graph(store=source.store, identifier=BNode("destination"))
    assert source.infer(inference="rdfs", destination=destination) is source
    assert (EX.x, RDF.type, EX.D) in destination
    assert (EX.x, RDF.type, EX.E) not in destination
    assert len(source) == 3


def test_dataset_inference_reads_union_and_preserves_contexts():
    dataset = StoreDataset()
    dataset.graph(EX.schema).add((EX.C, RDFS.subClassOf, EX.D))
    dataset.graph(EX.data).add((EX.x, RDF.type, EX.C))
    assert dataset.infer(inference="rdfs", destination=EX.destination) is dataset
    assert (EX.x, RDF.type, EX.D) in dataset.graph(EX.destination)
    assert len(dataset.graph(EX.schema)) == len(dataset.graph(EX.data)) == 1


def test_scoped_inference_reads_pending_changes_and_rolls_back_with_transaction():
    with Store() as store:
        original = Graph(store=store, identifier=EX.source)
        original.add((EX.C, RDFS.subClassOf, EX.D))
        with pytest.raises(RuntimeError, match="rollback"):
            with store.transaction() as transaction:
                graph = Graph(store=transaction, identifier=EX.source)
                graph.add((EX.x, RDF.type, EX.C))
                graph.infer(inference="rdfs")
                assert (EX.x, RDF.type, EX.D) in graph
                raise RuntimeError("rollback")
        assert len(original) == 1


@pytest.mark.parametrize("destination", [EX.destination, BNode("destination"), "urn:destination"])
def test_destination_identifiers_preserve_scope(destination):
    graph = Graph()
    graph.add((EX.C, RDFS.subClassOf, EX.D))
    graph.add((EX.x, RDF.type, EX.C))
    assert graph.infer(inference="rdfs", destination=destination) is graph
    target = Graph(store=graph.store, identifier=destination)
    assert (EX.x, RDF.type, EX.D) in target
    assert len(graph) == 2


def test_dataset_default_destination_does_not_copy_original_named_triples():
    dataset = StoreDataset()
    dataset.graph(EX.input).add((EX.C, RDFS.subClassOf, EX.D))
    dataset.graph(EX.input).add((EX.x, RDF.type, EX.C))
    dataset.infer(inference="rdfs")
    assert (EX.x, RDF.type, EX.D) in dataset.default_graph
    assert (EX.x, RDF.type, EX.C) not in dataset.default_graph
    assert len(dataset.graph(EX.input)) == 2


@pytest.mark.parametrize("destination", [Literal("wrong"), object(), ReferenceGraph()])
def test_invalid_destination_fails_without_changing_graph(destination):
    graph = Graph()
    graph.add((EX.x, RDF.type, EX.C))
    with pytest.raises((TypeError, ValueError)):
        graph.infer(inference="rdfs", destination=destination)
    assert len(graph) == 1


def test_native_failure_aborts_prior_transaction_changes_even_when_caught():
    with Store() as store:
        original = Graph(store=store, identifier=EX.source)
        original.add((EX.C, RDFS.subClassOf, EX.D))
        with pytest.raises(RuntimeError):
            with store.transaction() as transaction:
                graph = Graph(store=transaction, identifier=EX.source)
                graph.add((EX.x, RDF.type, EX.C))
                with pytest.raises(ValueError, match="Unknown inference mode"):
                    graph.infer(inference="unsupported")
        assert len(original) == 1


def test_malformed_rule_input_rolls_back_inferred_facts():
    graph = Graph()
    graph.add((EX.C, RDFS.subClassOf, EX.D))
    graph.add((EX.x, RDF.type, EX.C))
    head = BNode("broken")
    graph.add((EX.C, OWL.hasKey, head))
    graph.add((head, RDF.first, EX.key))
    graph.add((head, RDF.rest, head))
    before = set(graph)
    with pytest.raises(ValueError, match="recursive rdf:rest"):
        graph.infer(inference="both")
    assert set(graph) == before
    assert (EX.x, RDF.type, EX.D) not in graph


@pytest.mark.parametrize("relation", [OWL.hasKey, OWL.propertyChainAxiom])
@pytest.mark.parametrize("recursive", [False, True])
def test_list_failure_fixture_matches_original_semantics(relation, recursive):
    reference = ReferenceGraph()
    graph = Graph()
    head = BNode("failure-fixture")
    statements = [(EX.C, relation, head)]
    if recursive:
        statements.extend([(head, RDF.first, EX.key), (head, RDF.rest, head)])
    for statement in statements:
        reference.add(statement)
        graph.add(statement)
    # No literals occur here, so disable the reference's global converter swap.
    original = DeductiveClosure(OWLRL_Semantics, improved_datatypes=False)
    if recursive:
        with pytest.raises(ValueError, match="recursive rdf:rest"):
            original.expand(reference)
        with pytest.raises(ValueError, match="recursive rdf:rest"):
            graph.infer(inference="owlrl")
        assert set(graph) == set(statements)
    else:
        original.expand(reference)
        graph.infer(inference="owlrl")
        assert set(graph) == set(reference)


def test_scoped_inference_persists_without_leaking_other_graphs(tmp_path):
    path = tmp_path / "scoped-store"
    with Store(path) as store:
        graph = Graph(store=store, identifier=EX.source)
        graph.add((EX.C, RDFS.subClassOf, EX.D))
        graph.add((EX.x, RDF.type, EX.C))
        Graph(store=store, identifier=EX.outside).add((EX.D, RDFS.subClassOf, EX.Outside))
        graph.infer(inference="rdfs")
    with Store(path) as reopened:
        graph = Graph(store=reopened, identifier=EX.source)
        assert (EX.x, RDF.type, EX.D) in graph
        assert (EX.x, RDF.type, EX.Outside) not in graph


def test_python_destination_failure_aborts_prior_transaction_changes_when_caught():
    with Store() as store:
        original = Graph(store=store, identifier=EX.source)
        with pytest.raises(RuntimeError):
            with store.transaction() as transaction:
                graph = Graph(store=transaction, identifier=EX.source)
                graph.add((EX.x, RDF.type, EX.C))
                with pytest.raises(TypeError, match="destination"):
                    graph.infer(destination=object())
        assert len(original) == 0


@pytest.mark.parametrize("union", [False, True])
@pytest.mark.parametrize("transactional", [False, True])
def test_scoped_diagnostics_capture_context_and_retain_the_invocation(union, transactional):
    with Store() as store:

        def exercise(backend):
            source = (
                StoreDataset(store=backend) if union else Graph(store=backend, identifier=EX.source)
            )
            input_graph = source.graph(EX.source) if union else source
            input_graph.add((EX.x, EX.value, Literal("bad", datatype=XSD.decimal, normalize=False)))
            input_graph.add((EX.conflict, RDF.type, OWL.Nothing))
            with decimal.localcontext() as context:
                context.traps[decimal.InvalidOperation] = False
                context.clear_flags()
                assert source.infer(destination=EX.destination) is source
                assert context.flags[decimal.InvalidOperation]
            assert backend._last_inference.error_messages == [
                f"{EX.conflict} is defined of type 'Nothing'"
            ]
            destination = Graph(store=input_graph.store, identifier=EX.destination)
            error_type = Namespace("http://www.daml.org/2002/03/agents/agent-ont#").ErrorMessage
            assert list(destination.subjects(RDF.type, error_type))
            assert len(input_graph) == 2

        if transactional:
            with store.transaction() as transaction:
                exercise(transaction)
        else:
            exercise(store)
