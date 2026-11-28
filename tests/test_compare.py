"""``rdflib_turbo.compare`` is a native drop-in for ``rdflib.compare``."""

import pytest
import rdflib
import rdflib.compare as reference
from rdflib import BNode, Literal, URIRef
from rdflib.graph import ModificationException, ReadOnlyGraphAggregate
from rdflib_turbo import CanonicalizationBudgetExceeded, Dataset, Graph, compare

P = URIRef("urn:p")
CYCLE = "_:a <urn:p> _:b .\n_:b <urn:p> _:a .\n"
LOOPS = "_:a <urn:p> _:a .\n_:b <urn:p> _:b .\n"
GROUND = '<urn:s> <urn:p> "x" .\n'


def pair(document, factory=Graph):
    return factory().parse(data=document, format="nt")


@pytest.mark.parametrize(
    "left, right", [(CYCLE, CYCLE.replace("_:a", "_:z")), (CYCLE, LOOPS), (GROUND, CYCLE)]
)
@pytest.mark.parametrize("left_factory", [Graph, rdflib.Graph])
@pytest.mark.parametrize("right_factory", [Graph, rdflib.Graph])
def test_isomorphic_matches_rdflib_compare(left, right, left_factory, right_factory):
    expected = reference.isomorphic(pair(left, rdflib.Graph), pair(right, rdflib.Graph))
    one, other = pair(left, left_factory), pair(right, right_factory)
    assert compare.isomorphic(one, other) is expected
    if left_factory is Graph:
        assert one.isomorphic(other) is expected


def test_graph_isomorphic_has_no_false_positive_unlike_rdflib_graph_isomorphic():
    assert pair(CYCLE, rdflib.Graph).isomorphic(pair(LOOPS, rdflib.Graph)) is True
    assert pair(CYCLE).isomorphic(pair(LOOPS)) is False


def test_datasets_compare_graph_names():
    def dataset(name, factory=Dataset):
        result = factory()
        result.graph(URIRef(name)).add((BNode(), P, Literal("x")))
        return result

    assert compare.isomorphic(dataset("urn:g"), dataset("urn:g", rdflib.Dataset))
    assert not compare.isomorphic(dataset("urn:g"), dataset("urn:h"))
    assert dataset("urn:g").isomorphic(dataset("urn:g"))


def test_isomorphic_budget_is_caller_set():
    with pytest.raises(CanonicalizationBudgetExceeded):
        compare.isomorphic(pair(CYCLE), pair(CYCLE), work_budget=1)


def test_to_canonical_graph_is_a_read_only_aggregate_with_deterministic_blank_nodes():
    stats = {}
    one = compare.to_canonical_graph(pair(CYCLE), stats)
    other = compare.to_canonical_graph(pair(CYCLE.replace("_:a", "_:z"), rdflib.Graph))
    assert isinstance(one, ReadOnlyGraphAggregate)
    assert set(one) == set(other)
    assert {str(term) for triple in one for term in triple} == {"c14n0", "c14n1", "urn:p"}
    assert stats == {"triple_count": 2}
    with pytest.raises(ModificationException):
        one.add((URIRef("urn:s"), P, URIRef("urn:o")))
    with pytest.raises(TypeError, match=r"Dataset\.canonicalize"):
        compare.to_canonical_graph(Dataset())


def test_to_canonical_graph_keeps_exact_literal_identity():
    graph = Graph()
    graph.add((BNode(), P, Literal("a", datatype=rdflib.XSD.string)))
    (triple,) = compare.to_canonical_graph(graph)
    assert triple[2] == Literal("a", datatype=rdflib.XSD.string)


def test_graph_diff_partitions_canonical_triples():
    both, first, second = compare.graph_diff(pair(GROUND + CYCLE), pair(GROUND + LOOPS))
    assert set(both) == {(URIRef("urn:s"), P, Literal("x"))}
    assert len(first) == len(second) == 2
    assert set(first).isdisjoint(second)


def test_reference_names_are_the_original_objects():
    for name in ("IsomorphicGraph", "similar", "to_isomorphic"):
        assert getattr(compare, name) is getattr(reference, name)
    assert set(compare.__all__) == set(reference.__all__)
