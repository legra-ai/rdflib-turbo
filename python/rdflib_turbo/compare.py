"""Native drop-in for ``rdflib.compare``.

``isomorphic``, ``to_canonical_graph`` and ``graph_diff`` run RDFC-1.0 in the
native core; the remaining names are RDFLib's own objects. Canonical blank
nodes are RDFC-1.0 ``c14n`` labels rather than RDFLib's ``cb`` hash labels;
both are deterministic functions of the graph contents.
"""

from __future__ import annotations

from rdflib import Dataset as RDFLibDataset
from rdflib.compare import IsomorphicGraph, similar, to_isomorphic
from rdflib.graph import Graph as RDFLibGraph
from rdflib.graph import ReadOnlyGraphAggregate
from rdflib.term import BNode

from ._canonical import DEFAULT_WORK_BUDGET, canonical_store, exact_nquads

__all__ = [
    "IsomorphicGraph",
    "graph_diff",
    "isomorphic",
    "similar",
    "to_canonical_graph",
    "to_isomorphic",
]

Stats = dict[str, int | str]


def isomorphic(
    graph1: RDFLibGraph, graph2: RDFLibGraph, *, work_budget: int = DEFAULT_WORK_BUDGET
) -> bool:
    """Compare graphs for equality up to blank-node renaming.

    RDFLib graphs are explicitly copied into native storage first. Datasets
    are compared quad by quad, including graph names.
    """
    return exact_nquads(graph1, work_budget) == exact_nquads(graph2, work_budget)


def to_canonical_graph(
    g1: RDFLibGraph, stats: Stats | None = None, *, work_budget: int = DEFAULT_WORK_BUDGET
) -> ReadOnlyGraphAggregate:
    """Create a canonical, read-only graph with deterministic blank nodes."""
    from .graph import Graph, RDFLibStore
    from .store import Store

    if isinstance(g1, RDFLibDataset):
        raise TypeError("to_canonical_graph requires a Graph; use Dataset.canonicalize()")
    identifier = BNode()
    native = canonical_store(g1, work_budget)
    store = RDFLibStore(Store._from_native(native), default_identifier=identifier)
    graph = Graph(store=store, identifier=identifier, bind_namespaces="none")
    if stats is not None:
        stats["triple_count"] = len(graph)
    return ReadOnlyGraphAggregate([graph])


def graph_diff(
    g1: RDFLibGraph, g2: RDFLibGraph, *, work_budget: int = DEFAULT_WORK_BUDGET
) -> tuple[RDFLibGraph, RDFLibGraph, RDFLibGraph]:
    """Return the triples in both graphs, only in the first and only in the second."""
    cg1 = to_canonical_graph(g1, work_budget=work_budget)
    cg2 = to_canonical_graph(g2, work_budget=work_budget)
    return (cg1 * cg2, cg1 - cg2, cg2 - cg1)
