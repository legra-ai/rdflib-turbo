"""Standard graph reads reuse immutable identifiers within a bounded batch."""

from rdflib import Literal, URIRef
from rdflib_turbo import Graph


def test_standard_reads_share_repeated_predicate_objects_within_one_batch():
    graph = Graph()
    predicate = URIRef("urn:batch:predicate")
    for index in range(16):
        graph.add((URIRef(f"urn:batch:subject:{index}"), predicate, Literal(str(index))))

    rows = list(graph)
    assert len(rows) == 16
    assert all(value == predicate for _, value, _ in rows)
    assert len({id(value) for _, value, _ in rows}) == 1
