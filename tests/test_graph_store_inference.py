"""Graph views and inference share the transaction and native storage owner."""

import pytest
from rdflib import URIRef
from rdflib.namespace import RDF, RDFS
from rdflib_turbo import Graph, StoreDataset

CHILD, PARENT, SUBJECT = map(URIRef, ("urn:Child", "urn:Parent", "urn:subject"))
INFERRED = (SUBJECT, RDF.type, PARENT)


@pytest.mark.parametrize("commit", [False, True])
def test_graph_inference_observes_pending_writes_and_the_same_commit_boundary(commit):
    graph = Graph()
    try:
        with graph.transaction() as view:
            view.add((CHILD, RDFS.subClassOf, PARENT))
            view.add((SUBJECT, RDF.type, CHILD))
            view.store.backend.infer(inference="rdfs")
            assert INFERRED in view
            assert INFERRED not in graph
            assert len(graph) == 0
            if not commit:
                raise RuntimeError("abort the graph and inferred changes")
    except RuntimeError as error:
        assert not commit
        assert str(error) == "abort the graph and inferred changes"
    assert (INFERRED in graph) is commit
    if not commit:
        assert len(graph) == 0


def test_dataset_inference_reads_named_views_and_publishes_only_to_destination():
    dataset = StoreDataset()
    schema = dataset.graph(URIRef("urn:schema"))
    data = dataset.graph(URIRef("urn:data"))
    schema.add((CHILD, RDFS.subClassOf, PARENT))
    data.add((SUBJECT, RDF.type, CHILD))
    dataset.store.backend.infer(inference="rdfs", destination="urn:derived")
    assert INFERRED in dataset.graph(URIRef("urn:derived"))
    assert list(schema) == [(CHILD, RDFS.subClassOf, PARENT)]
    assert list(data) == [(SUBJECT, RDF.type, CHILD)]
    assert len(dataset.default_graph) == 0
