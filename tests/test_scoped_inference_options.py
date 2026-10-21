"""Scoped inference carries options and history through its native request."""

import owlrl
import pytest
from rdflib import BNode, Literal, Namespace
from rdflib import Dataset as ReferenceDataset
from rdflib import Graph as ReferenceGraph
from rdflib.compare import isomorphic
from rdflib.namespace import OWL, RDF, RDFS
from rdflib_turbo import Graph, Store, StoreDataset

EX = Namespace("urn:scoped-options:")


def source(kind, native):
    if kind == "dataset":
        graph = StoreDataset() if native else ReferenceDataset()
        context = graph.graph(EX.input)
    else:
        identifier = BNode("input") if kind == "blank" else EX.input
        graph = (
            Graph(bind_namespaces="none")
            if native and kind == "default"
            else Graph(Store(), identifier, bind_namespaces="none")
            if native
            else ReferenceGraph(identifier=identifier, bind_namespaces="none")
        )
        context = graph
    for triple in [
        (EX.C, RDFS.subClassOf, EX.D),
        (EX.x, RDF.type, EX.C),
        (EX.bad, RDF.type, OWL.Nothing),
    ]:
        context.add(triple)
    return graph


def ordinary(graph):
    result = ReferenceGraph()
    triples = (
        ((s, p, o) for s, p, o, _ in graph.quads())
        if isinstance(graph, ReferenceDataset)
        else graph
    )
    for triple in triples:
        if not isinstance(triple[0], Literal):
            result.add(triple)
    return result


@pytest.mark.parametrize("kind", ["default", "named", "blank", "dataset"])
@pytest.mark.parametrize(
    "mode,name",
    [("rdfs", "RDFS_Semantics"), ("owlrl", "OWLRL_Semantics"), ("both", "RDFS_OWLRL_Semantics")],
)
@pytest.mark.parametrize(
    "axioms,daxioms", [(False, False), (True, False), (False, True), (True, True)]
)
def test_scoped_flags_and_prior_history_match_original(kind, mode, name, axioms, daxioms):
    reference, graph = source(kind, False), source(kind, True)
    semantics = getattr(owlrl, name)(reference, axioms, daxioms)
    semantics.error_messages.extend(["retained", "retained"])
    semantics.closure()
    assert (
        graph.infer(
            inference=mode,
            axiomatic_triples=axioms,
            datatype_axioms=daxioms,
            prior_error_messages=["retained", "retained"],
        )
        is graph
    )
    assert graph.store.backend._last_inference.error_messages == semantics.error_messages
    assert isomorphic(ordinary(graph), ordinary(reference))


@pytest.mark.parametrize("kind", ["default", "named", "blank", "dataset"])
def test_scoped_options_stay_provisional_until_transaction_commit(kind):
    graph = source(kind, True)
    before = set(ordinary(graph))
    with pytest.raises(ValueError, match="abort"):
        with graph.transaction() as transaction:
            transaction.infer(
                axiomatic_triples=True, datatype_axioms=True, prior_error_messages=["retained"]
            )
            assert transaction.store.backend._last_inference.error_messages[0] == "retained"
            assert graph.store.backend._last_inference is None
            raise ValueError("abort")
    assert set(ordinary(graph)) == before
    assert graph.store.backend._last_inference is None
